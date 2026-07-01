//! Statement and expression checking.

use wscript_core::defs::{self, DefId, DefKind, VariantKind};
use wscript_core::span::Span;
use wscript_core::types::{FnSig, Type};

use crate::ast::*;

use super::methods::{self, SchemeConstraint};
use super::{
    BinOpKind, CallKind, Checker, ForKind, IndexKind, MethodRes, PathRes, PreludeFn, PrimKind,
    StructLitRes, TryKind, UnOpKind,
};

/// AST-depth budget for `check_expr` — the backstop behind the parser's
/// `MAX_NESTING_DEPTH`, sized so the checker stays well within the LSP's
/// smaller tokio stacks.
const MAX_EXPR_DEPTH: u32 = 500;

impl<'a> Checker<'a> {
    pub(crate) fn check_block(&mut self, block: &Block, expect: Option<&Type>) -> Type {
        self.push_scope();
        let n = block.stmts.len();
        let mut diverged = false;
        let mut tail: Option<Type> = None;
        for (i, stmt) in block.stmts.iter().enumerate() {
            let last = i + 1 == n;
            match stmt {
                Stmt::Let {
                    name,
                    ty,
                    init,
                    span,
                    id,
                } => {
                    let ann = ty.as_ref().map(|t| self.resolve_type(t));
                    let init_ty = match &ann {
                        Some(expected) => {
                            // Annotation is the source of truth; the
                            // initializer must fit it (incl. dyn coercion).
                            self.check_coerce(init, expected);
                            expected.clone()
                        }
                        None => self.check_expr(init, None),
                    };
                    let var_ty = ann.unwrap_or(init_ty.clone());
                    if matches!(self.resolve(&init_ty), Type::Never) {
                        diverged = true;
                    }
                    let local = self.declare_local(name, var_ty.clone());
                    self.out.decl_locals.insert(*id, local);
                    self.record_type(*id, var_ty);
                    self.require_resolved(*id, *span);
                }
                Stmt::LetElse {
                    pat,
                    init,
                    else_block,
                    span,
                    id,
                } => {
                    let init_ty = self.check_expr(init, None);
                    // The else block must diverge (PRD §3.4).
                    let else_ty = self.check_block(else_block, None);
                    if !matches!(self.resolve(&else_ty), Type::Never) {
                        let span = else_block.span;
                        self.error_help(
                            "E0222",
                            span,
                            "the `else` block of `let ... else` must diverge",
                            "end it with `return`, `break`, or `continue`",
                        );
                    }
                    if !self.pattern_is_refutable(pat, &init_ty) {
                        self.warn(
                            "W0001",
                            *span,
                            "irrefutable pattern in `let ... else`: the else block never runs",
                        );
                    }
                    // Bindings live in the enclosing scope.
                    self.check_pattern(pat, &init_ty);
                    self.record_type(*id, Type::Unit);
                }
                Stmt::Expr { expr, terminated } => {
                    if last && !*terminated {
                        let t = self.check_expr(expr, expect);
                        tail = Some(t);
                    } else {
                        let t = self.check_expr(expr, None);
                        if matches!(self.resolve(&t), Type::Never) {
                            diverged = true;
                        }
                    }
                }
            }
        }
        self.pop_scope();
        match tail {
            Some(t) => t,
            None if diverged => Type::Never,
            None => Type::Unit,
        }
    }

    pub(crate) fn resolve(&self, t: &Type) -> Type {
        self.infer.resolve(t)
    }

    fn require_resolved(&mut self, node: NodeId, span: Span) {
        self.must_resolve.push((node, span));
    }

    /// Check `e`, then make it fit `expected` (unification plus the one
    /// implicit coercion: concrete type → `dyn Trait` at typed boundaries,
    /// PRD §3.7).
    fn check_coerce(&mut self, e: &Expr, expected: &Type) -> Type {
        let found = self.check_expr(e, Some(expected));
        self.coerce(e.id, e.span, &found, expected);
        expected.clone()
    }

    pub(crate) fn coerce(&mut self, node: NodeId, span: Span, found: &Type, expected: &Type) {
        let exp = self.resolve(expected);
        let fnd = self.resolve(found);
        if let (Type::Dyn(trait_id), Type::Named(concrete)) = (&exp, &fnd) {
            let trait_id = *trait_id;
            let concrete = *concrete;
            match self.vtable_for(concrete, trait_id) {
                Some(vt) => {
                    self.out.dyn_wraps.insert(node, vt);
                }
                None => {
                    let ty_name = self.out.defs.name_of(concrete).to_string();
                    let tr_name = self.out.defs.name_of(trait_id).to_string();
                    self.error_help(
                        "E0223",
                        span,
                        format!("`{ty_name}` does not implement trait `{tr_name}`"),
                        format!("add `impl {tr_name} for {ty_name} {{ ... }}`"),
                    );
                }
            }
            return;
        }
        self.unify_or_err(
            expected,
            found,
            span,
            "the value's type must match what the context expects",
        );
    }

    pub(crate) fn check_expr(&mut self, e: &Expr, expect: Option<&Type>) -> Type {
        if self.expr_depth >= MAX_EXPR_DEPTH {
            if !self.expr_depth_reported {
                self.expr_depth_reported = true;
                self.error(
                    "E0271",
                    e.span,
                    format!("expression is nested more than {MAX_EXPR_DEPTH} levels deep"),
                );
            }
            return self.record_type(e.id, Type::Error);
        }
        self.expr_depth += 1;
        let ty = self.check_expr_inner(e, expect);
        self.expr_depth -= 1;
        self.record_type(e.id, ty)
    }

    fn check_expr_inner(&mut self, e: &Expr, expect: Option<&Type>) -> Type {
        match &e.kind {
            ExprKind::IntLit(_) => Type::Int,
            ExprKind::FloatLit(_) => Type::Float,
            ExprKind::BoolLit(_) => Type::Bool,
            ExprKind::CharLit(_) => Type::Char,
            ExprKind::StrLit(_) => Type::Str,
            ExprKind::UnitLit => Type::Unit,
            ExprKind::Error => Type::Error,
            ExprKind::Path(segments) => self.check_path_expr(e, segments),
            ExprKind::Unary { op, expr } => self.check_unary(e, *op, expr),
            ExprKind::Binary { op, lhs, rhs } => self.check_binary(e, *op, lhs, rhs),
            ExprKind::Assign { target, value } => self.check_assign(target, value),
            ExprKind::Call { callee, args } => self.check_call(e, callee, args),
            ExprKind::MethodCall { recv, name, args } => {
                self.check_method_call(e, recv, name, args)
            }
            ExprKind::Field { obj, name } => self.check_field(e, obj, name),
            ExprKind::Index { obj, idx } => self.check_index(e, obj, idx),
            ExprKind::StructLit { path, fields } => self.check_struct_lit(e, path, fields),
            ExprKind::ListLit(items) => self.check_list_lit(items, expect),
            ExprKind::MapLit(entries) => self.check_map_lit(e, entries, expect),
            ExprKind::If { cond, then, else_ } => {
                self.check_if(cond, then, else_.as_deref(), expect)
            }
            ExprKind::IfLet {
                pat,
                scrutinee,
                then,
                else_,
            } => self.check_if_let(pat, scrutinee, then, else_.as_deref(), expect),
            ExprKind::Match { scrutinee, arms } => self.check_match(e, scrutinee, arms, expect),
            ExprKind::While { cond, body } => {
                let cond_ty = self.check_expr(cond, Some(&Type::Bool));
                self.expect_bool(&cond_ty, cond.span, "a `while` condition");
                self.enter_loop();
                // Loop body values are discarded.
                self.check_block(body, None);
                self.exit_loop();
                Type::Unit
            }
            ExprKind::Loop { body } => {
                self.enter_loop();
                self.check_block(body, None);
                let has_break = self.exit_loop();
                if has_break { Type::Unit } else { Type::Never }
            }
            ExprKind::For { var, iter, body } => self.check_for(e, var, iter, body),
            ExprKind::Range { .. } => {
                self.error_help(
                    "E0225",
                    e.span,
                    "range expressions are only usable as `for` loop iterables in v1",
                    "write `for i in a..b { ... }`",
                );
                Type::Error
            }
            ExprKind::Break => {
                self.mark_break(e.span);
                Type::Never
            }
            ExprKind::Continue => {
                if !self.in_loop() {
                    self.error("E0221", e.span, "`continue` outside of a loop");
                }
                Type::Never
            }
            ExprKind::Return(value) => self.check_return(e, value.as_deref()),
            ExprKind::Block(b) => self.check_block(b, expect),
            ExprKind::Closure { params, ret, body } => {
                self.check_closure(e, params, ret.as_ref(), body, expect)
            }
            ExprKind::Try(inner) => self.check_try(e, inner),
        }
    }

    fn check_list_lit(&mut self, items: &[Expr], expect: Option<&Type>) -> Type {
        let elem = match expect.map(|t| self.resolve(t)) {
            Some(Type::List(e)) => *e,
            _ => self.infer.fresh(),
        };
        for item in items {
            self.check_coerce(item, &elem);
        }
        Type::List(Box::new(elem))
    }

    fn check_map_lit(&mut self, e: &Expr, entries: &[(Expr, Expr)], expect: Option<&Type>) -> Type {
        let (key, val) = match expect.map(|t| self.resolve(t)) {
            Some(Type::Map(k, v)) => (*k, *v),
            _ => (self.infer.fresh(), self.infer.fresh()),
        };
        for (k, v) in entries {
            self.check_coerce(k, &key);
            self.check_coerce(v, &val);
        }
        let kr = self.resolve(&key);
        if !matches!(
            kr,
            Type::Int | Type::Bool | Type::Char | Type::Str | Type::Error | Type::Var(_)
        ) {
            let span = entries.first().map(|(k, _)| k.span).unwrap_or(e.span);
            let ks = self.ty_str(&kr);
            self.error_help(
                "E0214",
                span,
                format!("`{ks}` cannot be a map key"),
                "map keys must be int, bool, char, or string",
            );
        }
        Type::Map(Box::new(key), Box::new(val))
    }

    fn check_if(
        &mut self,
        cond: &Expr,
        then: &Block,
        else_: Option<&Expr>,
        expect: Option<&Type>,
    ) -> Type {
        let cond_ty = self.check_expr(cond, Some(&Type::Bool));
        self.expect_bool(&cond_ty, cond.span, "an `if` condition");
        let then_ty = self.check_block(then, expect);
        match else_ {
            None => {
                let tt = self.resolve(&then_ty);
                if !matches!(tt, Type::Unit | Type::Never | Type::Error | Type::Var(_)) {
                    let span = then.span;
                    let ts = self.ty_str(&tt);
                    self.error_help(
                        "E0224",
                        span,
                        format!(
                            "`if` without `else` evaluates to unit, but the branch \
                             has type `{ts}`"
                        ),
                        "add an `else` branch, or discard the value",
                    );
                }
                Type::Unit
            }
            Some(else_expr) => {
                let else_ty = self.check_expr(else_expr, expect);
                self.combine_branches(&then_ty, &else_ty, else_expr.span)
            }
        }
    }

    fn check_if_let(
        &mut self,
        pat: &Pattern,
        scrutinee: &Expr,
        then: &Block,
        else_: Option<&Expr>,
        expect: Option<&Type>,
    ) -> Type {
        let scrut_ty = self.check_expr(scrutinee, None);
        if !self.pattern_is_refutable(pat, &scrut_ty) {
            self.warn(
                "W0001",
                pat.span,
                "irrefutable pattern in `if let`: the branch always runs",
            );
        }
        self.push_scope();
        self.check_pattern(pat, &scrut_ty);
        let then_ty = self.check_block(then, expect);
        self.pop_scope();
        match else_ {
            None => Type::Unit,
            Some(else_expr) => {
                let else_ty = self.check_expr(else_expr, expect);
                self.combine_branches(&then_ty, &else_ty, else_expr.span)
            }
        }
    }

    fn check_return(&mut self, e: &Expr, value: Option<&Expr>) -> Type {
        let ret = self.current_ret();
        match value {
            Some(v) => {
                self.check_coerce(v, &ret);
            }
            None => {
                let rr = self.resolve(&ret);
                if !matches!(rr, Type::Unit | Type::Error | Type::Never) {
                    let rs = self.ty_str(&rr);
                    self.error_help(
                        "E0226",
                        e.span,
                        format!("`return` without a value in a function returning `{rs}`"),
                        "write `return <value>`",
                    );
                }
            }
        }
        Type::Never
    }

    fn expect_bool(&mut self, t: &Type, span: Span, what: &str) {
        let r = self.resolve(t);
        if matches!(r, Type::Bool | Type::Error | Type::Never) {
            return;
        }
        if self.infer.unify(&Type::Bool, t).is_ok() {
            return;
        }
        let ts = self.ty_str(&r);
        self.error_help(
            "E0227",
            span,
            format!("{what} must be `bool`, found `{ts}`"),
            "wscript has no truthiness: write an explicit comparison",
        );
    }

    /// Result type of two branches, treating divergence properly.
    fn combine_branches(&mut self, a: &Type, b: &Type, span: Span) -> Type {
        let ra = self.resolve(a);
        let rb = self.resolve(b);
        if matches!(ra, Type::Never) {
            return rb;
        }
        if matches!(rb, Type::Never) {
            return ra;
        }
        self.unify_or_err(
            &ra,
            &rb,
            span,
            "all branches of an `if`/`match` expression must have the same type",
        );
        self.resolve(&ra)
    }

    // ------------------------------------------------------------- paths

    fn check_path_expr(&mut self, e: &Expr, segments: &[Ident]) -> Type {
        match segments {
            [single] => {
                if let Some((res, ty)) = self.lookup_var(&single.name) {
                    self.out.var_refs.insert(e.id, res);
                    if let Some(span) = self.lookup_var_span(&single.name) {
                        self.out.def_spans.insert(e.id, span);
                    }
                    return ty;
                }
                if single.name == "self" {
                    self.error_help(
                        "E0228",
                        e.span,
                        "`self` is only available inside methods",
                        "methods are functions in `impl` blocks whose first parameter is `self`",
                    );
                    return Type::Error;
                }
                if let Some((host_fn, konst)) = self.imported(&single.name) {
                    if let Some((ty, c)) = konst {
                        self.out.paths.insert(e.id, PathRes::Const(c));
                        return ty;
                    }
                    if host_fn.is_some() {
                        self.error_help(
                            "E0229",
                            e.span,
                            "host functions cannot be used as values in v1",
                            "call it directly, or wrap it in a closure: `|x| f(x)`",
                        );
                        return Type::Error;
                    }
                }
                if let Some(proto) = self.fn_by_name(&single.name) {
                    self.out.paths.insert(e.id, PathRes::FnValue(proto));
                    let info = &self.out.fn_infos[proto as usize];
                    self.out.def_spans.insert(e.id, info.span);
                    let sig = info.sig.clone();
                    return Type::Fn(Box::new(sig));
                }
                if single.name == "None" {
                    self.out.paths.insert(
                        e.id,
                        PathRes::Variant {
                            def: defs::DEF_OPTION,
                            tag: defs::TAG_NONE,
                        },
                    );
                    return Type::Option(Box::new(self.infer.fresh()));
                }
                if Self::prelude_fn(&single.name).is_some() {
                    self.error_help(
                        "E0229",
                        e.span,
                        format!("`{}` is a built-in function, not a value", single.name),
                        "call it directly, or wrap it in a closure",
                    );
                    return Type::Error;
                }
                let name = single.name.clone();
                self.error_unknown_value(e.span, &name);
                Type::Error
            }
            [first, second] => {
                // module::item
                if let Some(mod_idx) = self.module_idx(&first.name) {
                    return self.check_module_item(e, mod_idx, second, /*as_value=*/ true);
                }
                // Enum::Variant
                if let Some(def) = self.enum_by_name(&first.name) {
                    return self.unit_variant_value(e, def, second);
                }
                if self.module_is_registered(&first.name) {
                    let name = first.name.clone();
                    self.error_help(
                        "E0230",
                        first.span,
                        format!("module `{name}` is not imported"),
                        format!("add `use {name}` at the top of the script"),
                    );
                    return Type::Error;
                }
                let name = first.name.clone();
                self.error_unknown_value(first.span, &name);
                Type::Error
            }
            [first, second, third] => {
                // module::Enum::Variant — types are ambient, so the module
                // qualifier is accepted but the enum resolves by name.
                if self.module_idx(&first.name).is_some() || self.module_is_registered(&first.name)
                {
                    if let Some(def) = self.enum_by_name(&second.name) {
                        return self.unit_variant_value(e, def, third);
                    }
                    let name = second.name.clone();
                    self.error("E0212", second.span, format!("unknown type `{name}`"));
                    return Type::Error;
                }
                let name = first.name.clone();
                self.error_unknown_value(first.span, &name);
                Type::Error
            }
            _ => {
                self.error("E0231", e.span, "path has too many segments");
                Type::Error
            }
        }
    }

    fn error_unknown_value(&mut self, span: Span, name: &str) {
        self.error_help(
            "E0230",
            span,
            format!("cannot find value `{name}` in this scope"),
            "check the spelling; variables must be declared with `let` before use",
        );
    }

    pub(crate) fn enum_by_name(&self, name: &str) -> Option<DefId> {
        let id = self.type_name(name)?;
        self.out.defs.as_enum(id)?;
        Some(id)
    }

    fn module_item_lookup(
        &self,
        mod_idx: usize,
        name: &str,
    ) -> Option<Result<(FnSig, u32), (Type, wscript_core::bytecode::Const)>> {
        let module = &self.reg.modules[mod_idx];
        if let Some((_, sig, idx, _)) = module.fns.iter().find(|(n, ..)| n == name) {
            return Some(Ok((sig.clone(), *idx)));
        }
        if let Some((_, ty, c)) = module.consts.iter().find(|(n, ..)| n == name) {
            return Some(Err((ty.clone(), c.clone())));
        }
        None
    }

    fn check_module_item(
        &mut self,
        e: &Expr,
        mod_idx: usize,
        item: &Ident,
        as_value: bool,
    ) -> Type {
        match self.module_item_lookup(mod_idx, &item.name) {
            Some(Ok(_)) => {
                if as_value {
                    self.error_help(
                        "E0229",
                        e.span,
                        "host functions cannot be used as values in v1",
                        "call it directly, or wrap it in a closure: `|x| f(x)`",
                    );
                    Type::Error
                } else {
                    unreachable!("call paths handled in check_call")
                }
            }
            Some(Err((ty, c))) => {
                self.out.paths.insert(e.id, PathRes::Const(c));
                ty
            }
            None => {
                let module = self.reg.modules[mod_idx].name.clone();
                let name = item.name.clone();
                self.error_help(
                    "E0201",
                    item.span,
                    format!("module `{module}` has no item `{name}`"),
                    "check the module's `.wscripti` interface for available items",
                );
                Type::Error
            }
        }
    }

    /// `Enum::Variant` used as a value (unit variants only).
    fn unit_variant_value(&mut self, e: &Expr, def: DefId, variant: &Ident) -> Type {
        let Some((tag, vdef_kind, n_fields)) = self.variant_info(def, &variant.name) else {
            let enum_name = self.out.defs.name_of(def).to_string();
            let vname = variant.name.clone();
            self.error_help(
                "E0232",
                variant.span,
                format!("enum `{enum_name}` has no variant `{vname}`"),
                "check the enum declaration for the available variants",
            );
            return Type::Error;
        };
        match vdef_kind {
            VariantKind::Unit => {
                self.out.paths.insert(e.id, PathRes::Variant { def, tag });
                self.enum_value_type(def)
            }
            VariantKind::Tuple => {
                let vname = variant.name.clone();
                self.error_help(
                    "E0233",
                    e.span,
                    format!("variant `{vname}` takes a payload"),
                    format!(
                        "write `{}::{vname}({})`",
                        self.out.defs.name_of(def),
                        (0..n_fields).map(|_| "...").collect::<Vec<_>>().join(", ")
                    ),
                );
                Type::Error
            }
            VariantKind::Struct => {
                let vname = variant.name.clone();
                self.error_help(
                    "E0233",
                    e.span,
                    format!("variant `{vname}` has named fields"),
                    format!("write `{}::{vname} {{ ... }}`", self.out.defs.name_of(def)),
                );
                Type::Error
            }
        }
    }

    pub(crate) fn variant_info(&self, def: DefId, name: &str) -> Option<(u32, VariantKind, usize)> {
        let ed = self.out.defs.as_enum(def)?;
        let (tag, v) = ed
            .variants
            .iter()
            .enumerate()
            .find(|(_, v)| v.name == name)?;
        Some((tag as u32, v.kind, v.fields.len()))
    }

    /// The script-level type of a value of enum `def`. `Option`/`Result`
    /// instantiate fresh payload vars.
    pub(crate) fn enum_value_type(&mut self, def: DefId) -> Type {
        if def == defs::DEF_OPTION {
            Type::Option(Box::new(self.infer.fresh()))
        } else if def == defs::DEF_RESULT {
            Type::Result(Box::new(self.infer.fresh()), Box::new(self.infer.fresh()))
        } else {
            Type::Named(def)
        }
    }

    /// Payload field types of `def::variant`, instantiated against the
    /// scrutinee/constructed type when the enum is Option/Result.
    pub(crate) fn variant_payload_types(&self, def: DefId, tag: u32, enum_ty: &Type) -> Vec<Type> {
        let Some(ed) = self.out.defs.as_enum(def) else {
            return vec![];
        };
        let fields: Vec<Type> = ed.variants[tag as usize]
            .fields
            .iter()
            .map(|(_, t)| t.clone())
            .collect();
        let args: Vec<Type> = match self.resolve(enum_ty) {
            Type::Option(t) => vec![*t],
            Type::Result(t, e) => vec![*t, *e],
            _ => return fields,
        };
        fields
            .iter()
            .map(|t| super::subst_params(t, &args))
            .collect()
    }

    // --------------------------------------------------------- operators

    fn check_unary(&mut self, e: &Expr, op: UnOp, operand: &Expr) -> Type {
        let t = self.check_expr(operand, None);
        let rt = self.resolve(&t);
        match op {
            UnOp::Not => {
                self.expect_bool(&rt, operand.span, "the operand of `!`");
                self.out.un_ops.insert(e.id, UnOpKind::Not);
                Type::Bool
            }
            UnOp::Neg => match rt {
                Type::Int => {
                    self.out.un_ops.insert(e.id, UnOpKind::NegInt);
                    Type::Int
                }
                Type::Float => {
                    self.out.un_ops.insert(e.id, UnOpKind::NegFloat);
                    Type::Float
                }
                Type::Var(_) => {
                    // Default unresolved numeric negation to int.
                    if self.infer.unify(&Type::Int, &t).is_ok() {
                        self.out.un_ops.insert(e.id, UnOpKind::NegInt);
                        Type::Int
                    } else {
                        Type::Error
                    }
                }
                Type::Named(def) => {
                    if let Some(protos) = self.trait_impls.get(&(def, defs::TRAIT_NEG)) {
                        let proto = protos[0];
                        self.out.un_ops.insert(e.id, UnOpKind::NegCall { proto });
                        Type::Named(def)
                    } else {
                        let name = self.out.defs.name_of(def).to_string();
                        self.error_help(
                            "E0234",
                            e.span,
                            format!("cannot negate `{name}`"),
                            format!("implement the `Neg` trait: `impl Neg for {name}`"),
                        );
                        Type::Error
                    }
                }
                Type::Error | Type::Never => Type::Error,
                other => {
                    let ts = self.ty_str(&other);
                    self.error("E0234", e.span, format!("cannot negate `{ts}`"));
                    Type::Error
                }
            },
        }
    }

    fn check_binary(&mut self, e: &Expr, op: BinOp, lhs: &Expr, rhs: &Expr) -> Type {
        use BinOp::*;
        match op {
            And | Or => {
                let lt = self.check_expr(lhs, Some(&Type::Bool));
                self.expect_bool(&lt, lhs.span, "the left operand of a logical operator");
                let rt = self.check_expr(rhs, Some(&Type::Bool));
                self.expect_bool(&rt, rhs.span, "the right operand of a logical operator");
                self.out.bin_ops.insert(
                    e.id,
                    if op == And {
                        BinOpKind::And
                    } else {
                        BinOpKind::Or
                    },
                );
                Type::Bool
            }
            Add | Sub | Mul | Div | Rem => self.check_arith(e, op, lhs, rhs),
            Eq | Ne => self.check_eq(e, op == Ne, lhs, rhs),
            Lt | Le | Gt | Ge => self.check_cmp(e, op, lhs, rhs),
        }
    }

    fn check_arith(&mut self, e: &Expr, op: BinOp, lhs: &Expr, rhs: &Expr) -> Type {
        let lt = self.check_expr(lhs, None);
        let rt = self.check_expr(rhs, Some(&lt));
        self.unify_or_err(
            &lt,
            &rt,
            rhs.span,
            "arithmetic requires both operands to have the same type \
             (use `int(x)` / `float(x)` to convert)",
        );
        let t = self.resolve(&lt);
        match &t {
            Type::Int => {
                self.out.bin_ops.insert(e.id, BinOpKind::IntArith(op));
                Type::Int
            }
            Type::Float => {
                self.out.bin_ops.insert(e.id, BinOpKind::FloatArith(op));
                Type::Float
            }
            Type::Str if op == BinOp::Add => {
                self.out.bin_ops.insert(e.id, BinOpKind::Concat);
                Type::Str
            }
            Type::Var(_) => {
                // Unconstrained operands (e.g. closure params used only
                // here) default to int.
                if self.infer.unify(&Type::Int, &t).is_ok() {
                    self.out.bin_ops.insert(e.id, BinOpKind::IntArith(op));
                    Type::Int
                } else {
                    Type::Error
                }
            }
            Type::Named(def) => {
                let def = *def;
                let trait_id = match op {
                    BinOp::Add => defs::TRAIT_ADD,
                    BinOp::Sub => defs::TRAIT_SUB,
                    BinOp::Mul => defs::TRAIT_MUL,
                    BinOp::Div => defs::TRAIT_DIV,
                    _ => defs::TRAIT_REM,
                };
                if let Some(protos) = self.trait_impls.get(&(def, trait_id)) {
                    let proto = protos[0];
                    self.out
                        .bin_ops
                        .insert(e.id, BinOpKind::ArithCall { proto });
                    Type::Named(def)
                } else {
                    let name = self.out.defs.name_of(def).to_string();
                    let tr = self.out.defs.name_of(trait_id).to_string();
                    self.error_help(
                        "E0234",
                        e.span,
                        format!("no `{}` operator for `{name}`", op_symbol(op)),
                        format!("implement the `{tr}` trait: `impl {tr} for {name}`"),
                    );
                    Type::Error
                }
            }
            Type::Error | Type::Never => Type::Error,
            other => {
                let ts = self.ty_str(other);
                let help = if matches!(other, Type::Str) {
                    "strings support `+` for concatenation only"
                } else {
                    "arithmetic operators work on int and float \
                     (and types implementing the operator traits)"
                };
                self.error_help(
                    "E0234",
                    e.span,
                    format!("no `{}` operator for `{ts}`", op_symbol(op)),
                    help,
                );
                Type::Error
            }
        }
    }

    fn check_eq(&mut self, e: &Expr, negate: bool, lhs: &Expr, rhs: &Expr) -> Type {
        let lt = self.check_expr(lhs, None);
        let rt = self.check_expr(rhs, Some(&lt));
        self.unify_or_err(
            &lt,
            &rt,
            rhs.span,
            "both sides of a comparison must have the same type",
        );
        let t = self.resolve(&lt);
        let kind = match &t {
            Type::Int => Some(PrimKind::Int),
            Type::Float => Some(PrimKind::Float),
            Type::Bool => Some(PrimKind::Bool),
            Type::Char => Some(PrimKind::Char),
            Type::Str => Some(PrimKind::Str),
            _ => None,
        };
        if let Some(kind) = kind {
            self.out
                .bin_ops
                .insert(e.id, BinOpKind::EqPrim { kind, negate });
            return Type::Bool;
        }
        match &t {
            Type::Named(def) => {
                let def = *def;
                if let Some(&proto) = self.out.impl_maps.eq.get(&def.0) {
                    self.out
                        .bin_ops
                        .insert(e.id, BinOpKind::EqCall { proto, negate });
                    Type::Bool
                } else if self.named_has_eq(def) {
                    self.out.bin_ops.insert(e.id, BinOpKind::EqValue { negate });
                    Type::Bool
                } else {
                    let name = self.out.defs.name_of(def).to_string();
                    self.error_help(
                        "E0235",
                        e.span,
                        format!("`==` on `{name}` requires an `Eq` implementation"),
                        format!(
                            "add `#[derive(Eq)]` to `{name}`, or `impl Eq for {name}`; \
                             for reference identity use `same(a, b)` (PRD §3.7)"
                        ),
                    );
                    Type::Error
                }
            }
            Type::Option(_) | Type::Result(..) | Type::List(_) | Type::Map(..) => {
                if self.eq_able(&t) {
                    self.out.bin_ops.insert(e.id, BinOpKind::EqValue { negate });
                    Type::Bool
                } else {
                    let ts = self.ty_str(&t);
                    self.error_help(
                        "E0235",
                        e.span,
                        format!("`==` on `{ts}` requires the element type to support `==`"),
                        "element types must be primitives, strings, or Eq types",
                    );
                    Type::Error
                }
            }
            Type::Unit => {
                self.error_help(
                    "E0235",
                    e.span,
                    "cannot compare unit values",
                    "`unit` has only one value; the comparison is always true",
                );
                Type::Error
            }
            Type::Error | Type::Never | Type::Var(_) => {
                // Unconstrained: accept and lower to structural equality.
                self.out.bin_ops.insert(e.id, BinOpKind::EqValue { negate });
                Type::Bool
            }
            other => {
                let ts = self.ty_str(other);
                self.error_help(
                    "E0235",
                    e.span,
                    format!("`==` is not supported for `{ts}`"),
                    "function, weak and dyn values support `same(a, b)` reference \
                     identity only",
                );
                Type::Error
            }
        }
    }

    fn check_cmp(&mut self, e: &Expr, op: BinOp, lhs: &Expr, rhs: &Expr) -> Type {
        let lt = self.check_expr(lhs, None);
        let rt = self.check_expr(rhs, Some(&lt));
        self.unify_or_err(
            &lt,
            &rt,
            rhs.span,
            "both sides of a comparison must have the same type",
        );
        let t = self.resolve(&lt);
        let kind = match &t {
            Type::Int => Some(PrimKind::Int),
            Type::Float => Some(PrimKind::Float),
            Type::Char => Some(PrimKind::Char),
            Type::Str => Some(PrimKind::Str),
            _ => None,
        };
        if let Some(kind) = kind {
            self.out
                .bin_ops
                .insert(e.id, BinOpKind::CmpPrim { kind, op });
            return Type::Bool;
        }
        match &t {
            Type::Var(_) => {
                if self.infer.unify(&Type::Int, &t).is_ok() {
                    self.out.bin_ops.insert(
                        e.id,
                        BinOpKind::CmpPrim {
                            kind: PrimKind::Int,
                            op,
                        },
                    );
                    Type::Bool
                } else {
                    Type::Error
                }
            }
            Type::Named(def) => {
                let def = *def;
                if let Some(&proto) = self.out.impl_maps.cmp.get(&def.0) {
                    self.out
                        .bin_ops
                        .insert(e.id, BinOpKind::CmpCall { proto, op });
                    Type::Bool
                } else if self.derives.get(&def).is_some_and(|d| d.ord) {
                    self.out.bin_ops.insert(e.id, BinOpKind::CmpValue { op });
                    Type::Bool
                } else {
                    let name = self.out.defs.name_of(def).to_string();
                    self.error_help(
                        "E0235",
                        e.span,
                        format!("ordering comparison on `{name}` requires `Ord`"),
                        format!("add `#[derive(Eq, Ord)]` to `{name}`, or `impl Ord for {name}`"),
                    );
                    Type::Error
                }
            }
            Type::Error | Type::Never => Type::Error,
            other => {
                let ts = self.ty_str(other);
                self.error(
                    "E0235",
                    e.span,
                    format!("ordering comparison is not supported for `{ts}`"),
                );
                Type::Error
            }
        }
    }

    // ------------------------------------------------------- assignments

    fn check_assign(&mut self, target: &Expr, value: &Expr) -> Type {
        match &target.kind {
            ExprKind::Path(segments) if segments.len() == 1 => {
                let target_ty = self.check_expr(target, None);
                if self.out.var_refs.contains_key(&target.id) {
                    self.check_coerce(value, &target_ty);
                } else if !matches!(target_ty, Type::Error) {
                    self.error_help(
                        "E0236",
                        target.span,
                        "invalid assignment target",
                        "only variables, fields, and list/map elements can be assigned",
                    );
                    self.check_expr(value, None);
                }
            }
            ExprKind::Field { .. } => {
                let field_ty = self.check_expr(target, None);
                self.check_coerce(value, &field_ty);
            }
            ExprKind::Index { .. } => {
                let elem_ty = self.check_expr(target, None);
                if let Some(IndexKind::UserGet { .. }) = self.out.indexes.get(&target.id) {
                    self.error_help(
                        "E0236",
                        target.span,
                        "cannot assign through a user `Index` impl",
                        "the `Index` trait is read-only in v1",
                    );
                }
                self.check_coerce(value, &elem_ty);
            }
            _ => {
                self.error_help(
                    "E0236",
                    target.span,
                    "invalid assignment target",
                    "only variables, fields, and list/map elements can be assigned",
                );
                self.check_expr(target, None);
                self.check_expr(value, None);
            }
        }
        Type::Unit
    }

    // ------------------------------------------------------------- calls

    fn check_call(&mut self, e: &Expr, callee: &Expr, args: &[Expr]) -> Type {
        // Path callees resolve to functions/constructors; anything else is
        // a function value.
        if let ExprKind::Path(segments) = &callee.kind {
            if let Some((kind, ret)) = self.resolve_call_path(e, callee, segments, args) {
                self.out.calls.insert(e.id, kind);
                return ret;
            }
            return Type::Error;
        }
        let callee_ty = self.check_expr(callee, None);
        self.check_value_call(e, &callee_ty, callee.span, args)
    }

    fn check_value_call(
        &mut self,
        e: &Expr,
        callee_ty: &Type,
        callee_span: Span,
        args: &[Expr],
    ) -> Type {
        let t = self.resolve(callee_ty);
        match t {
            Type::Fn(sig) => {
                self.check_args(e.span, "this function", &sig.params, args);
                self.out.calls.insert(e.id, CallKind::Value);
                sig.ret.clone()
            }
            Type::Error | Type::Never => Type::Error,
            other => {
                let ts = self.ty_str(&other);
                self.error_help(
                    "E0237",
                    callee_span,
                    format!("`{ts}` is not callable"),
                    "only functions and closures can be called",
                );
                for a in args {
                    self.check_expr(a, None);
                }
                Type::Error
            }
        }
    }

    fn check_args(&mut self, call_span: Span, what: &str, params: &[Type], args: &[Expr]) {
        if params.len() != args.len() {
            self.error_help(
                "E0238",
                call_span,
                format!(
                    "{what} takes {} argument{}, found {}",
                    params.len(),
                    if params.len() == 1 { "" } else { "s" },
                    args.len()
                ),
                "check the function's signature",
            );
        }
        for (i, a) in args.iter().enumerate() {
            match params.get(i) {
                Some(p) => {
                    self.check_coerce(a, &p.clone());
                }
                None => {
                    self.check_expr(a, None);
                }
            }
        }
    }

    /// Resolve a call whose callee is a path. Returns (kind, return type),
    /// or None after reporting an error.
    fn resolve_call_path(
        &mut self,
        e: &Expr,
        callee: &Expr,
        segments: &[Ident],
        args: &[Expr],
    ) -> Option<(CallKind, Type)> {
        match segments {
            [single] => {
                // Locals (closure values) shadow functions.
                if let Some((res, ty)) = self.lookup_var(&single.name) {
                    self.out.var_refs.insert(callee.id, res);
                    if let Some(span) = self.lookup_var_span(&single.name) {
                        self.out.def_spans.insert(callee.id, span);
                    }
                    self.record_type(callee.id, ty.clone());
                    let ret = self.check_value_call(e, &ty, callee.span, args);
                    return Some((CallKind::Value, ret));
                }
                if let Some((host_fn, _)) = self.imported(&single.name)
                    && let Some(idx) = host_fn
                {
                    let sig = self.reg.host_fns[idx as usize].sig.clone();
                    self.check_args(e.span, &format!("`{}`", single.name), &sig.params, args);
                    return Some((CallKind::Host(idx), sig.ret));
                }
                if let Some(proto) = self.fn_by_name(&single.name) {
                    let info = &self.out.fn_infos[proto as usize];
                    self.out.def_spans.insert(callee.id, info.span);
                    let sig = info.sig.clone();
                    self.check_args(e.span, &format!("`{}`", single.name), &sig.params, args);
                    return Some((CallKind::Proto(proto), sig.ret));
                }
                // Ambient Option/Result constructors.
                match single.name.as_str() {
                    "Some" => {
                        let t = self.infer.fresh();
                        self.check_args(e.span, "`Some`", std::slice::from_ref(&t), args);
                        return Some((
                            CallKind::Variant {
                                def: defs::DEF_OPTION,
                                tag: defs::TAG_SOME,
                            },
                            Type::Option(Box::new(t)),
                        ));
                    }
                    "Ok" => {
                        let t = self.infer.fresh();
                        self.check_args(e.span, "`Ok`", std::slice::from_ref(&t), args);
                        return Some((
                            CallKind::Variant {
                                def: defs::DEF_RESULT,
                                tag: defs::TAG_OK,
                            },
                            Type::Result(Box::new(t), Box::new(self.infer.fresh())),
                        ));
                    }
                    "Err" => {
                        let t = self.infer.fresh();
                        self.check_args(e.span, "`Err`", std::slice::from_ref(&t), args);
                        return Some((
                            CallKind::Variant {
                                def: defs::DEF_RESULT,
                                tag: defs::TAG_ERR,
                            },
                            Type::Result(Box::new(self.infer.fresh()), Box::new(t)),
                        ));
                    }
                    _ => {}
                }
                if let Some(p) = Self::prelude_fn(&single.name) {
                    let ret = self.check_prelude_call(e, p, args)?;
                    return Some((CallKind::Prelude(p), ret));
                }
                let name = single.name.clone();
                self.error_help(
                    "E0230",
                    single.span,
                    format!("cannot find function `{name}`"),
                    "functions must be declared in the script or imported from a \
                     registered module",
                );
                None
            }
            [first, second] => {
                if let Some(mod_idx) = self.module_idx(&first.name) {
                    match self.module_item_lookup(mod_idx, &second.name) {
                        Some(Ok((sig, idx))) => {
                            self.check_args(
                                e.span,
                                &format!("`{}::{}`", first.name, second.name),
                                &sig.params,
                                args,
                            );
                            return Some((CallKind::Host(idx), sig.ret));
                        }
                        Some(Err((ty, _))) => {
                            let ts = self.ty_str(&ty);
                            self.error_help(
                                "E0237",
                                e.span,
                                format!(
                                    "`{}::{}` is a constant of type `{ts}`, not a function",
                                    first.name, second.name
                                ),
                                "remove the call parentheses",
                            );
                            return None;
                        }
                        None => {
                            let module = first.name.clone();
                            let name = second.name.clone();
                            self.error_help(
                                "E0201",
                                second.span,
                                format!("module `{module}` has no item `{name}`"),
                                "check the module's `.wscripti` interface for available items",
                            );
                            return None;
                        }
                    }
                }
                if let Some(def) = self.enum_by_name(&first.name) {
                    return self.check_variant_ctor(e, def, second, args);
                }
                if self.module_is_registered(&first.name) {
                    let name = first.name.clone();
                    self.error_help(
                        "E0230",
                        first.span,
                        format!("module `{name}` is not imported"),
                        format!("add `use {name}` at the top of the script"),
                    );
                    return None;
                }
                let name = first.name.clone();
                self.error_unknown_value(first.span, &name);
                None
            }
            [first, second, third] => {
                if (self.module_idx(&first.name).is_some()
                    || self.module_is_registered(&first.name))
                    && let Some(def) = self.enum_by_name(&second.name)
                {
                    return self.check_variant_ctor(e, def, third, args);
                }
                let name = first.name.clone();
                self.error_unknown_value(first.span, &name);
                None
            }
            _ => {
                self.error("E0231", e.span, "path has too many segments");
                None
            }
        }
    }

    fn check_variant_ctor(
        &mut self,
        e: &Expr,
        def: DefId,
        variant: &Ident,
        args: &[Expr],
    ) -> Option<(CallKind, Type)> {
        let Some((tag, kind, _)) = self.variant_info(def, &variant.name) else {
            let enum_name = self.out.defs.name_of(def).to_string();
            let vname = variant.name.clone();
            self.error(
                "E0232",
                variant.span,
                format!("enum `{enum_name}` has no variant `{vname}`"),
            );
            return None;
        };
        match kind {
            VariantKind::Tuple => {
                let result_ty = self.enum_value_type(def);
                let payload = self.variant_payload_types(def, tag, &result_ty);
                self.check_args(
                    e.span,
                    &format!("variant `{}`", variant.name),
                    &payload,
                    args,
                );
                Some((CallKind::Variant { def, tag }, result_ty))
            }
            VariantKind::Unit => {
                let vname = variant.name.clone();
                self.error_help(
                    "E0233",
                    e.span,
                    format!("variant `{vname}` takes no payload"),
                    format!(
                        "write `{}::{vname}` without parentheses",
                        self.out.defs.name_of(def)
                    ),
                );
                None
            }
            VariantKind::Struct => {
                let vname = variant.name.clone();
                self.error_help(
                    "E0233",
                    e.span,
                    format!("variant `{vname}` has named fields"),
                    format!(
                        "write `{}::{vname} {{ field: value, ... }}`",
                        self.out.defs.name_of(def)
                    ),
                );
                None
            }
        }
    }

    /// Type-check a prelude (builtin) call. Returns the call's type.
    fn check_prelude_call(&mut self, e: &Expr, p: PreludeFn, args: &[Expr]) -> Option<Type> {
        let arity_err = |me: &mut Self, name: &str, n: &str| {
            me.error_help(
                "E0238",
                e.span,
                format!("`{name}` takes {n}"),
                "see the language tour for the prelude functions",
            );
        };
        match p {
            PreludeFn::Print | PreludeFn::Str => {
                let name = if p == PreludeFn::Print {
                    "print"
                } else {
                    "str"
                };
                if args.len() != 1 {
                    arity_err(self, name, "exactly one argument");
                }
                for a in args {
                    self.check_expr(a, None);
                }
                Some(if p == PreludeFn::Print {
                    Type::Unit
                } else {
                    Type::Str
                })
            }
            PreludeFn::Println => {
                if args.len() > 1 {
                    arity_err(self, "println", "zero or one arguments");
                }
                for a in args {
                    self.check_expr(a, None);
                }
                Some(Type::Unit)
            }
            PreludeFn::Fmt => {
                if args.is_empty() {
                    arity_err(self, "fmt", "a template string plus arguments");
                    return Some(Type::Str);
                }
                let t0 = self.check_expr(&args[0], Some(&Type::Str));
                self.unify_or_err(
                    &Type::Str,
                    &t0,
                    args[0].span,
                    "the first argument of `fmt` is the template string",
                );
                for a in &args[1..] {
                    self.check_expr(a, None);
                }
                // If the template is a literal, validate the placeholder
                // count right here at compile time.
                if let ExprKind::StrLit(template) = &args[0].kind {
                    let placeholders = count_placeholders(template);
                    if placeholders != args.len() - 1 {
                        let span = args[0].span;
                        self.error_help(
                            "E0239",
                            span,
                            format!(
                                "format template has {placeholders} `{{}}` placeholder{} \
                                 but {} argument{} given",
                                if placeholders == 1 { "" } else { "s" },
                                args.len() - 1,
                                if args.len() - 1 == 1 {
                                    " was"
                                } else {
                                    "s were"
                                }
                            ),
                            "each `{}` consumes one argument; escape literal braces as \
                             `{{` and `}}`",
                        );
                    }
                }
                Some(Type::Str)
            }
            PreludeFn::Same => {
                if args.len() != 2 {
                    arity_err(self, "same", "exactly two arguments");
                    for a in args {
                        self.check_expr(a, None);
                    }
                    return Some(Type::Bool);
                }
                let t0 = self.check_expr(&args[0], None);
                let _t1 = self.check_expr(&args[1], Some(&t0));
                Some(Type::Bool)
            }
            PreludeFn::Weak => {
                if args.len() != 1 {
                    arity_err(self, "weak", "exactly one argument");
                    return Some(Type::Error);
                }
                let t = self.check_expr(&args[0], None);
                let rt = self.resolve(&t);
                if !self.is_reference_type(&rt) || matches!(rt, Type::Option(_) | Type::Result(..))
                {
                    let ts = self.ty_str(&rt);
                    self.error_help(
                        "E0213",
                        args[0].span,
                        format!("cannot create a weak reference to `{ts}`"),
                        "weak references apply to structs, enums, List, Map, and functions \
                         (PRD §4.2)",
                    );
                    return Some(Type::Error);
                }
                Some(Type::Weak(Box::new(rt)))
            }
            PreludeFn::Int => {
                if args.len() != 1 {
                    arity_err(self, "int", "exactly one argument");
                    return Some(Type::Int);
                }
                let t = self.check_expr(&args[0], None);
                let rt = self.resolve(&t);
                if !matches!(
                    rt,
                    Type::Int | Type::Float | Type::Char | Type::Error | Type::Never
                ) {
                    let ts = self.ty_str(&rt);
                    self.error_help(
                        "E0240",
                        args[0].span,
                        format!("`int()` cannot convert from `{ts}`"),
                        "int() accepts int, float (truncates), and char (code point); \
                         to parse a string use `s.parse_int()`",
                    );
                }
                Some(Type::Int)
            }
            PreludeFn::Float => {
                if args.len() != 1 {
                    arity_err(self, "float", "exactly one argument");
                    return Some(Type::Float);
                }
                let t = self.check_expr(&args[0], None);
                let rt = self.resolve(&t);
                if !matches!(rt, Type::Int | Type::Float | Type::Error | Type::Never) {
                    let ts = self.ty_str(&rt);
                    self.error_help(
                        "E0240",
                        args[0].span,
                        format!("`float()` cannot convert from `{ts}`"),
                        "float() accepts int and float; to parse a string use \
                         `s.parse_float()`",
                    );
                }
                Some(Type::Float)
            }
        }
    }

    // ------------------------------------------------------ method calls

    fn check_method_call(&mut self, e: &Expr, recv: &Expr, name: &Ident, args: &[Expr]) -> Type {
        let recv_ty = self.check_expr(recv, None);
        let rt = self.resolve(&recv_ty);
        match &rt {
            Type::Error | Type::Never => Type::Error,
            Type::Var(_) => {
                self.error_help(
                    "E0241",
                    recv.span,
                    "cannot call a method on a value of unknown type",
                    "add a type annotation so the receiver's type is known here",
                );
                Type::Error
            }
            Type::Named(def) => self.check_named_method(e, *def, name, args),
            Type::Dyn(trait_id) => self.check_dyn_method(e, *trait_id, name, args),
            other => {
                // Builtin container/string/Option/Result/weak methods.
                match methods::builtin_method(other, &name.name) {
                    Some(scheme) => {
                        self.apply_scheme(e, other, &name.name, scheme, name.span, args)
                    }
                    None => {
                        let ts = self.ty_str(other);
                        let mname = name.name.clone();
                        self.error_help(
                            "E0241",
                            name.span,
                            format!("no method `{mname}` on `{ts}`"),
                            "see the stdlib reference for the built-in methods of this type",
                        );
                        for a in args {
                            self.check_expr(a, None);
                        }
                        Type::Error
                    }
                }
            }
        }
    }

    fn apply_scheme(
        &mut self,
        e: &Expr,
        recv_ty: &Type,
        mname: &str,
        scheme: methods::Scheme,
        name_span: Span,
        args: &[Expr],
    ) -> Type {
        // Receiver type parameters + fresh vars for scheme-local params.
        let mut subst: Vec<Type> = match recv_ty {
            Type::List(t) => vec![(**t).clone()],
            Type::Map(k, v) => vec![(**k).clone(), (**v).clone()],
            Type::Option(t) => vec![(**t).clone()],
            Type::Result(t, err) => vec![(**t).clone(), (**err).clone()],
            Type::Weak(t) => vec![(**t).clone()],
            _ => vec![],
        };
        for _ in 0..scheme.fresh {
            let v = self.infer.fresh();
            subst.push(v);
        }
        let params: Vec<Type> = scheme
            .params
            .iter()
            .map(|p| super::subst_params(p, &subst))
            .collect();
        let ret = super::subst_params(&scheme.ret, &subst);
        self.check_args(e.span, &format!("`{mname}`"), &params, args);
        // Element constraints (e.g. `contains` needs comparable elements).
        if let Some(c) = scheme.constraint {
            let elem = self.resolve(subst.first().unwrap_or(&Type::Error));
            let ok = match c {
                SchemeConstraint::EqElem => self.eq_able(&elem),
                SchemeConstraint::OrdElem => {
                    matches!(
                        elem,
                        Type::Int | Type::Float | Type::Char | Type::Str | Type::Error
                    )
                }
                SchemeConstraint::StrElem => {
                    matches!(elem, Type::Str | Type::Error) || matches!(elem, Type::Var(_))
                }
            };
            if !ok {
                let es = self.ty_str(&elem);
                let (msg, help): (String, &str) = match c {
                    SchemeConstraint::EqElem => (
                        format!("`{mname}` requires `{es}` elements to support `==`"),
                        "element types must be primitives, strings, or Eq types",
                    ),
                    SchemeConstraint::OrdElem => (
                        format!("`{mname}` requires orderable elements, but found `{es}`"),
                        "sortable element types are int, float, char, and string",
                    ),
                    SchemeConstraint::StrElem => (
                        format!("`{mname}` requires `List[string]`, but elements are `{es}`"),
                        "use `.map(...)` to convert elements to strings first",
                    ),
                };
                self.error_help("E0242", name_span, msg, help);
            }
        }
        self.out
            .methods
            .insert(e.id, MethodRes::Builtin(scheme.builtin));
        ret
    }

    fn check_named_method(&mut self, e: &Expr, def: DefId, name: &Ident, args: &[Expr]) -> Type {
        // 1. Inherent script methods.
        if let Some(&proto) = self.inherent.get(&def).and_then(|m| m.get(&name.name)) {
            let sig = self.out.fn_infos[proto as usize].sig.clone();
            self.check_args(e.span, &format!("`{}`", name.name), &sig.params[1..], args);
            self.out.methods.insert(e.id, MethodRes::Proto(proto));
            return sig.ret;
        }
        // 2. Host-registered methods.
        if let Some(ms) = self.reg.methods.get(&def)
            && let Some(m) = ms.iter().find(|m| m.name == name.name)
        {
            let sig = m.sig.clone();
            let idx = m.host_idx;
            self.check_args(e.span, &format!("`{}`", name.name), &sig.params, args);
            self.out.methods.insert(e.id, MethodRes::Host(idx));
            return sig.ret;
        }
        // 3. Trait-impl methods (static dispatch on the concrete type).
        let mut candidates: Vec<(DefId, usize, u32)> = Vec::new();
        for (&(ty, trait_id), protos) in &self.trait_impls {
            if ty != def {
                continue;
            }
            if let Some(td) = self.out.defs.as_trait(trait_id)
                && let Some(slot) = td.methods.iter().position(|(n, _)| *n == name.name)
            {
                candidates.push((trait_id, slot, protos[slot]));
            }
        }
        if candidates.len() > 1 {
            let traits: Vec<String> = candidates
                .iter()
                .map(|(t, ..)| self.out.defs.name_of(*t).to_string())
                .collect();
            let mname = name.name.clone();
            self.error_help(
                "E0243",
                name.span,
                format!("ambiguous method `{mname}`"),
                format!(
                    "implemented by multiple traits: {}; rename one of the trait methods",
                    traits.join(", ")
                ),
            );
            return Type::Error;
        }
        if let Some((_, _, proto)) = candidates.pop() {
            let sig = self.out.fn_infos[proto as usize].sig.clone();
            self.check_args(e.span, &format!("`{}`", name.name), &sig.params[1..], args);
            self.out.methods.insert(e.id, MethodRes::Proto(proto));
            return sig.ret;
        }
        // 4. Derived clone.
        if name.name == "clone" && self.derives.get(&def).is_some_and(|d| d.clone) {
            self.check_args(e.span, "`clone`", &[], args);
            self.out
                .methods
                .insert(e.id, MethodRes::Builtin(wscript_core::Builtin::DeepClone));
            return Type::Named(def);
        }
        let ty_name = self.out.defs.name_of(def).to_string();
        let mname = name.name.clone();
        let help = if name.name == "clone" {
            format!("add `#[derive(Clone)]` to `{ty_name}` to enable deep cloning")
        } else {
            format!("no inherent, host, or trait method `{mname}` is defined for `{ty_name}`")
        };
        self.error_help(
            "E0241",
            name.span,
            format!("no method `{mname}` on `{ty_name}`"),
            help,
        );
        for a in args {
            self.check_expr(a, None);
        }
        Type::Error
    }

    fn check_dyn_method(&mut self, e: &Expr, trait_id: DefId, name: &Ident, args: &[Expr]) -> Type {
        let Some(td) = self.out.defs.as_trait(trait_id).cloned() else {
            return Type::Error;
        };
        let Some(slot) = td.methods.iter().position(|(n, _)| *n == name.name) else {
            let tr = td.name.clone();
            let mname = name.name.clone();
            self.error_help(
                "E0241",
                name.span,
                format!("no method `{mname}` on `dyn {tr}`"),
                format!(
                    "trait `{tr}` declares: {}",
                    td.methods
                        .iter()
                        .map(|(n, _)| n.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            );
            for a in args {
                self.check_expr(a, None);
            }
            return Type::Error;
        };
        let sig = td.methods[slot].1.clone();
        self.check_args(e.span, &format!("`{}`", name.name), &sig.params, args);
        self.out
            .methods
            .insert(e.id, MethodRes::Virtual { slot: slot as u16 });
        sig.ret
    }

    // ---------------------------------------------------- fields & index

    fn check_field(&mut self, e: &Expr, obj: &Expr, name: &Ident) -> Type {
        let obj_ty = self.check_expr(obj, None);
        let rt = self.resolve(&obj_ty);
        match &rt {
            Type::Named(def) => match self.out.defs.get(*def) {
                DefKind::Struct(s) => {
                    if s.opaque {
                        let ty_name = s.name.clone();
                        self.error_help(
                            "E0244",
                            name.span,
                            format!(
                                "`{ty_name}` is an opaque host type: fields are not accessible"
                            ),
                            "opaque types expose methods only (PRD §6.2)",
                        );
                        return Type::Error;
                    }
                    match s.fields.iter().position(|(n, _)| *n == name.name) {
                        Some(idx) => {
                            let ty = s.fields[idx].1.clone();
                            self.out.fields.insert(e.id, idx as u16);
                            ty
                        }
                        None => {
                            let ty_name = s.name.clone();
                            let avail: Vec<String> =
                                s.fields.iter().map(|(n, _)| n.clone()).collect();
                            let fname = name.name.clone();
                            self.error_help(
                                "E0244",
                                name.span,
                                format!("no field `{fname}` on `{ty_name}`"),
                                if avail.is_empty() {
                                    format!("`{ty_name}` has no fields")
                                } else {
                                    format!("available fields: {}", avail.join(", "))
                                },
                            );
                            Type::Error
                        }
                    }
                }
                DefKind::Enum(en) => {
                    let ty_name = en.name.clone();
                    let fname = name.name.clone();
                    self.error_help(
                        "E0244",
                        name.span,
                        format!("cannot access field `{fname}` on enum `{ty_name}`"),
                        "destructure the enum with `match` or `if let` to reach variant fields",
                    );
                    Type::Error
                }
                DefKind::Trait(_) => Type::Error,
            },
            Type::Error | Type::Never => Type::Error,
            other => {
                let ts = self.ty_str(other);
                let fname = name.name.clone();
                self.error_help(
                    "E0244",
                    name.span,
                    format!("`{ts}` has no field `{fname}`"),
                    "only struct values have fields; did you mean a method call \
                     `.{name}()`?"
                        .replace("{name}", &fname),
                );
                Type::Error
            }
        }
    }

    fn check_index(&mut self, e: &Expr, obj: &Expr, idx: &Expr) -> Type {
        let obj_ty = self.check_expr(obj, None);
        let rt = self.resolve(&obj_ty);
        match &rt {
            Type::List(elem) => {
                let it = self.check_expr(idx, Some(&Type::Int));
                self.unify_or_err(&Type::Int, &it, idx.span, "list indices are `int`");
                self.out.indexes.insert(e.id, IndexKind::List);
                (**elem).clone()
            }
            Type::Map(k, v) => {
                self.check_coerce(idx, &k.clone());
                self.out.indexes.insert(e.id, IndexKind::Map);
                (**v).clone()
            }
            Type::Str => {
                self.error_help(
                    "E0245",
                    e.span,
                    "strings cannot be indexed directly",
                    "use `s.chars()` for a List[char] or `s.slice(start, end)` for a \
                     substring",
                );
                self.check_expr(idx, None);
                Type::Error
            }
            Type::Named(def) => {
                let def = *def;
                if let Some(protos) = self.trait_impls.get(&(def, defs::TRAIT_INDEX)) {
                    let proto = protos[0];
                    let sig = self.out.fn_infos[proto as usize].sig.clone();
                    // sig.params[0] = receiver, [1] = index type.
                    let idx_ty = sig.params.get(1).cloned().unwrap_or(Type::Error);
                    self.check_coerce(idx, &idx_ty);
                    self.out.indexes.insert(e.id, IndexKind::UserGet { proto });
                    sig.ret
                } else {
                    let name = self.out.defs.name_of(def).to_string();
                    self.error_help(
                        "E0245",
                        e.span,
                        format!("`{name}` does not support indexing"),
                        format!("implement the `Index` trait: `impl Index for {name}`"),
                    );
                    self.check_expr(idx, None);
                    Type::Error
                }
            }
            Type::Error | Type::Never => {
                self.check_expr(idx, None);
                Type::Error
            }
            other => {
                let ts = self.ty_str(other);
                self.error("E0245", e.span, format!("`{ts}` does not support indexing"));
                self.check_expr(idx, None);
                Type::Error
            }
        }
    }

    // ----------------------------------------------------- struct literal

    fn check_struct_lit(&mut self, e: &Expr, path: &[Ident], fields: &[(Ident, Expr)]) -> Type {
        // Resolve the path: `Type { .. }` or `Enum::Variant { .. }` (with
        // an optional leading module qualifier on the enum).
        let (def, variant): (DefId, Option<&Ident>) = match path {
            [ty] => match self.type_name(&ty.name) {
                Some(def) => (def, None),
                None => {
                    let name = ty.name.clone();
                    self.error("E0212", ty.span, format!("unknown type `{name}`"));
                    self.check_lit_fields_poison(fields);
                    return Type::Error;
                }
            },
            [en, variant] => match self.enum_by_name(&en.name) {
                Some(def) => (def, Some(variant)),
                None => {
                    let name = en.name.clone();
                    self.error("E0212", en.span, format!("unknown enum `{name}`"));
                    self.check_lit_fields_poison(fields);
                    return Type::Error;
                }
            },
            [_module, en, variant] => match self.enum_by_name(&en.name) {
                Some(def) => (def, Some(variant)),
                None => {
                    let name = en.name.clone();
                    self.error("E0212", en.span, format!("unknown enum `{name}`"));
                    self.check_lit_fields_poison(fields);
                    return Type::Error;
                }
            },
            _ => {
                self.error("E0231", e.span, "path has too many segments");
                self.check_lit_fields_poison(fields);
                return Type::Error;
            }
        };

        let (decl_fields, lit_res, result_ty): (Vec<(String, Type)>, StructLitRes, Type) =
            match variant {
                None => match self.out.defs.get(def) {
                    DefKind::Struct(s) => {
                        if s.opaque {
                            let name = s.name.clone();
                            self.error_help(
                                "E0246",
                                e.span,
                                format!(
                                    "`{name}` is an opaque host type and cannot be \
                                     constructed in script"
                                ),
                                "opaque values are created by host functions (PRD §6.2)",
                            );
                            self.check_lit_fields_poison(fields);
                            return Type::Error;
                        }
                        (
                            s.fields.clone(),
                            StructLitRes::Struct(def),
                            Type::Named(def),
                        )
                    }
                    DefKind::Enum(en) => {
                        let name = en.name.clone();
                        self.error_help(
                            "E0246",
                            e.span,
                            format!("`{name}` is an enum, not a struct"),
                            format!("construct a variant: `{name}::Variant {{ ... }}`"),
                        );
                        self.check_lit_fields_poison(fields);
                        return Type::Error;
                    }
                    DefKind::Trait(_) => {
                        self.check_lit_fields_poison(fields);
                        return Type::Error;
                    }
                },
                Some(v) => {
                    let Some((tag, kind, _)) = self.variant_info(def, &v.name) else {
                        let enum_name = self.out.defs.name_of(def).to_string();
                        let vname = v.name.clone();
                        self.error(
                            "E0232",
                            v.span,
                            format!("enum `{enum_name}` has no variant `{vname}`"),
                        );
                        self.check_lit_fields_poison(fields);
                        return Type::Error;
                    };
                    if kind != VariantKind::Struct {
                        let vname = v.name.clone();
                        self.error_help(
                            "E0233",
                            e.span,
                            format!("variant `{vname}` does not have named fields"),
                            "use parentheses for tuple variants, or no payload for unit \
                             variants",
                        );
                        self.check_lit_fields_poison(fields);
                        return Type::Error;
                    }
                    let result_ty = self.enum_value_type(def);
                    let names: Vec<(String, Type)> = self
                        .out
                        .defs
                        .as_enum(def)
                        .map(|ed| ed.variants[tag as usize].fields.clone())
                        .unwrap_or_default();
                    let payload = self.variant_payload_types(def, tag, &result_ty);
                    let decl: Vec<(String, Type)> =
                        names.iter().map(|(n, _)| n.clone()).zip(payload).collect();
                    (decl, StructLitRes::Variant { def, tag }, result_ty)
                }
            };

        // Every declared field exactly once.
        let mut provided: Vec<Option<()>> = vec![None; decl_fields.len()];
        let mut order: Vec<u16> = Vec::with_capacity(fields.len());
        for (fname, value) in fields {
            match decl_fields.iter().position(|(n, _)| *n == fname.name) {
                Some(idx) => {
                    if provided[idx].replace(()).is_some() {
                        let n = fname.name.clone();
                        self.error("E0247", fname.span, format!("field `{n}` set twice"));
                    }
                    order.push(idx as u16);
                    let expected = decl_fields[idx].1.clone();
                    self.check_coerce(value, &expected);
                }
                None => {
                    let n = fname.name.clone();
                    let ty_name = self.out.defs.name_of(def).to_string();
                    let avail: Vec<String> = decl_fields.iter().map(|(n, _)| n.clone()).collect();
                    self.error_help(
                        "E0247",
                        fname.span,
                        format!("`{ty_name}` has no field `{n}`"),
                        format!("available fields: {}", avail.join(", ")),
                    );
                    order.push(u16::MAX);
                    self.check_expr(value, None);
                }
            }
        }
        let missing: Vec<String> = decl_fields
            .iter()
            .enumerate()
            .filter(|(i, _)| provided[*i].is_none())
            .map(|(_, (n, _))| n.clone())
            .collect();
        if !missing.is_empty() {
            let ty_name = self.out.defs.name_of(def).to_string();
            self.error_help(
                "E0247",
                e.span,
                format!(
                    "missing fields in `{ty_name}` literal: {}",
                    missing.join(", ")
                ),
                "every field must be initialized",
            );
        }
        self.out.struct_lits.insert(e.id, lit_res);
        self.out.field_orders.insert(e.id, order);
        result_ty
    }

    fn check_lit_fields_poison(&mut self, fields: &[(Ident, Expr)]) {
        for (_, value) in fields {
            self.check_expr(value, None);
        }
    }

    // ---------------------------------------------------------- for / try

    fn check_for(&mut self, e: &Expr, var: &Ident, iter: &Expr, body: &Block) -> Type {
        // Ranges are handled as a `for` header form, not a value.
        let (kind, elem_ty) = if let ExprKind::Range { lo, hi, inclusive } = &iter.kind {
            let lt = self.check_expr(lo, Some(&Type::Int));
            self.unify_or_err(&Type::Int, &lt, lo.span, "range bounds are `int`");
            let ht = self.check_expr(hi, Some(&Type::Int));
            self.unify_or_err(&Type::Int, &ht, hi.span, "range bounds are `int`");
            self.record_type(iter.id, Type::Int);
            (
                if *inclusive {
                    ForKind::RangeInclusive
                } else {
                    ForKind::RangeExclusive
                },
                Type::Int,
            )
        } else {
            let it = self.check_expr(iter, None);
            match self.resolve(&it) {
                Type::List(t) => (ForKind::List, *t),
                Type::Map(k, _) => (ForKind::MapKeys, *k),
                Type::Str => (ForKind::StrChars, Type::Char),
                Type::Error | Type::Never => (ForKind::List, Type::Error),
                other => {
                    let ts = self.ty_str(&other);
                    self.error_help(
                        "E0248",
                        iter.span,
                        format!("`{ts}` is not iterable"),
                        "`for` iterates over ranges (a..b), List (elements), Map (keys), \
                         and string (chars)",
                    );
                    (ForKind::List, Type::Error)
                }
            }
        };
        self.out.for_kinds.insert(e.id, kind);
        self.push_scope();
        let local = self.declare_local(var, elem_ty);
        self.out.decl_locals.insert(e.id, local);
        self.enter_loop();
        self.check_block(body, None);
        self.exit_loop();
        self.pop_scope();
        Type::Unit
    }

    fn check_try(&mut self, e: &Expr, inner: &Expr) -> Type {
        let t = self.check_expr(inner, None);
        let rt = self.resolve(&t);
        let ret = self.resolve(&self.current_ret());
        match rt {
            Type::Option(payload) => {
                if !matches!(ret, Type::Option(_) | Type::Error) {
                    let rs = self.ty_str(&ret);
                    self.error_help(
                        "E0249",
                        e.span,
                        format!(
                            "`?` on an Option requires the function to return Option, \
                             but it returns `{rs}`"
                        ),
                        "change the return type to `Option[...]`, or handle the None case \
                         with `match`/`if let`",
                    );
                }
                self.out.try_kinds.insert(e.id, TryKind::Option);
                *payload
            }
            Type::Result(payload, err) => {
                match &ret {
                    Type::Result(_, ret_err) => {
                        self.unify_or_err(
                            ret_err,
                            &err,
                            e.span,
                            "the error type propagated by `?` must match the function's \
                             error type",
                        );
                    }
                    Type::Error => {}
                    other => {
                        let rs = self.ty_str(other);
                        self.error_help(
                            "E0249",
                            e.span,
                            format!(
                                "`?` on a Result requires the function to return Result, \
                                 but it returns `{rs}`"
                            ),
                            "change the return type to `Result[..., ...]`, or handle the \
                             Err case with `match`",
                        );
                    }
                }
                self.out.try_kinds.insert(e.id, TryKind::Result);
                *payload
            }
            Type::Error | Type::Never => Type::Error,
            other => {
                let ts = self.ty_str(&other);
                self.error_help(
                    "E0249",
                    e.span,
                    format!("`?` requires an Option or Result, found `{ts}`"),
                    "the `?` operator early-returns None/Err (PRD §3.5)",
                );
                Type::Error
            }
        }
    }

    // ----------------------------------------------------------- closures

    fn check_closure(
        &mut self,
        e: &Expr,
        params: &[(Ident, Option<TypeExpr>)],
        ret_ann: Option<&TypeExpr>,
        body: &Expr,
        expect: Option<&Type>,
    ) -> Type {
        // Parameter types: annotation > expectation > fresh var.
        let expected_sig = match expect.map(|t| self.resolve(t)) {
            Some(Type::Fn(sig)) => Some(sig),
            _ => None,
        };
        let mut param_tys = Vec::with_capacity(params.len());
        for (i, (_, ann)) in params.iter().enumerate() {
            let ty = match ann {
                Some(t) => self.resolve_type(t),
                None => match expected_sig.as_ref().and_then(|s| s.params.get(i)) {
                    Some(t) => t.clone(),
                    None => self.infer.fresh(),
                },
            };
            param_tys.push(ty);
        }
        let ret_ty = match ret_ann {
            Some(t) => self.resolve_type(t),
            None => match expected_sig.as_ref() {
                Some(s) => s.ret.clone(),
                None => self.infer.fresh(),
            },
        };
        if let Some(s) = &expected_sig
            && s.params.len() != params.len()
        {
            self.error_help(
                "E0238",
                e.span,
                format!(
                    "closure takes {} parameter{}, but the context expects {}",
                    params.len(),
                    if params.len() == 1 { "" } else { "s" },
                    s.params.len()
                ),
                "match the expected function signature",
            );
        }

        let sig = FnSig::new(param_tys.clone(), ret_ty.clone());
        let proto = self.begin_closure(e.id, sig.clone(), e.span);
        self.set_closure_ret(ret_ty.clone());
        for ((name, _), ty) in params.iter().zip(&param_tys) {
            self.declare_local(name, ty.clone());
        }
        let body_ty = self.check_expr(body, Some(&ret_ty));
        let body_rt = self.resolve(&body_ty);
        if !matches!(body_rt, Type::Never) {
            self.unify_or_err(
                &ret_ty,
                &body_ty,
                body.span,
                "the closure body must produce the closure's return type",
            );
        }
        self.end_closure(proto);

        // Unresolved parameter types are an error: inference is local.
        for ((name, _), ty) in params.iter().zip(&param_tys) {
            if self.infer.contains_unbound(ty) {
                let n = name.name.clone();
                self.error_help(
                    "E0250",
                    name.span,
                    format!("cannot infer the type of closure parameter `{n}`"),
                    "add an annotation: `|x: int| ...` (closure parameters are inferred \
                     only where the context determines them, PRD §3.3)",
                );
            }
        }

        self.out.closures.insert(e.id, super::ClosureRes { proto });
        Type::Fn(Box::new(FnSig::new(param_tys, self.resolve(&ret_ty))))
    }
}

fn op_symbol(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Rem => "%",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::And => "&&",
        BinOp::Or => "||",
    }
}

/// Count `{}` placeholders, honouring `{{`/`}}` escapes.
pub(crate) fn count_placeholders(template: &str) -> usize {
    let bytes = template.as_bytes();
    let mut count = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                i += 2;
                continue;
            }
            if i + 1 < bytes.len() && bytes[i + 1] == b'}' {
                count += 1;
                i += 2;
                continue;
            }
        }
        i += 1;
    }
    count
}
