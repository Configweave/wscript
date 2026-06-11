//! Golden tests parser → AST (PRD §11): small programs with their exact
//! AST shape pinned, plus diagnostic snapshots for recovery behaviour.
//!
//! The goldens are compact structural renderings (not full Debug dumps) so
//! they stay reviewable; any parser change that reshapes the tree shows up
//! as a readable diff here.

use wisp_compiler::ast::*;

fn render(file: &SourceFile) -> String {
    let mut out = String::new();
    for item in &file.items {
        render_item(item, &mut out, 0);
    }
    out
}

fn pad(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn render_item(item: &Item, out: &mut String, depth: usize) {
    pad(out, depth);
    match item {
        Item::Use(u) => {
            out.push_str(&format!(
                "use {}{}\n",
                u.module.name,
                u.item
                    .as_ref()
                    .map(|i| format!("::{}", i.name))
                    .unwrap_or_default()
            ));
        }
        Item::Fn(f) => {
            let params: Vec<String> = f
                .params
                .iter()
                .map(|p| {
                    if p.is_self {
                        "self".into()
                    } else {
                        format!("{}:{}", p.name.name, render_ty(p.ty.as_ref()))
                    }
                })
                .collect();
            out.push_str(&format!(
                "fn {}({}) -> {}{}\n",
                f.name.name,
                params.join(", "),
                render_ty(f.ret.as_ref()),
                if f.has_body { "" } else { " <decl>" }
            ));
            render_block(&f.body, out, depth + 1);
        }
        Item::Struct(s) => {
            out.push_str(&format!(
                "struct {} [{}]{}\n",
                s.name.name,
                s.derives
                    .iter()
                    .map(|d| d.name.clone())
                    .collect::<Vec<_>>()
                    .join(","),
                if s.opaque { " opaque" } else { "" }
            ));
            for f in &s.fields {
                pad(out, depth + 1);
                out.push_str(&format!("{}: {}\n", f.name.name, render_ty(Some(&f.ty))));
            }
        }
        Item::Enum(e) => {
            out.push_str(&format!("enum {}\n", e.name.name));
            for v in &e.variants {
                pad(out, depth + 1);
                match &v.body {
                    VariantBody::Unit => out.push_str(&format!("{}\n", v.name.name)),
                    VariantBody::Tuple(tys) => out.push_str(&format!(
                        "{}({})\n",
                        v.name.name,
                        tys.iter()
                            .map(|t| render_ty(Some(t)))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )),
                    VariantBody::Struct(fs) => out.push_str(&format!(
                        "{} {{ {} }}\n",
                        v.name.name,
                        fs.iter()
                            .map(|f| format!("{}: {}", f.name.name, render_ty(Some(&f.ty))))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )),
                }
            }
        }
        Item::Trait(t) => {
            out.push_str(&format!("trait {}\n", t.name.name));
            for m in &t.methods {
                pad(out, depth + 1);
                out.push_str(&format!(
                    "fn {}({}) -> {}\n",
                    m.name.name,
                    m.params
                        .iter()
                        .map(|p| format!("{}:{}", p.name.name, render_ty(p.ty.as_ref())))
                        .collect::<Vec<_>>()
                        .join(", "),
                    render_ty(m.ret.as_ref())
                ));
            }
        }
        Item::Impl(im) => {
            match &im.trait_name {
                Some(tr) => out.push_str(&format!("impl {} for {}\n", tr.name, im.ty_name.name)),
                None => out.push_str(&format!("impl {}\n", im.ty_name.name)),
            }
            for f in &im.fns {
                render_item_fn_shallow(f, out, depth + 1);
            }
        }
        Item::Mod(m) => {
            out.push_str(&format!("mod {}\n", m.name.name));
            for item in &m.items {
                render_item(item, out, depth + 1);
            }
        }
        Item::Const(c) => {
            out.push_str(&format!(
                "const {}: {}\n",
                c.name.name,
                render_ty(Some(&c.ty))
            ));
        }
    }
}

fn render_item_fn_shallow(f: &FnDecl, out: &mut String, depth: usize) {
    pad(out, depth);
    out.push_str(&format!("fn {}(..)\n", f.name.name));
    render_block(&f.body, out, depth + 1);
}

fn render_block(b: &Block, out: &mut String, depth: usize) {
    for stmt in &b.stmts {
        match stmt {
            Stmt::Let { name, ty, init, .. } => {
                pad(out, depth);
                out.push_str(&format!("let {}:{} =\n", name.name, render_ty(ty.as_ref())));
                render_expr(init, out, depth + 1);
            }
            Stmt::LetElse {
                pat,
                init,
                else_block,
                ..
            } => {
                pad(out, depth);
                out.push_str(&format!("let-else {} =\n", render_pat(pat)));
                render_expr(init, out, depth + 1);
                pad(out, depth);
                out.push_str("else\n");
                render_block(else_block, out, depth + 1);
            }
            Stmt::Expr { expr, terminated } => {
                pad(out, depth);
                out.push_str(if *terminated { "expr; \n" } else { "expr\n" });
                render_expr(expr, out, depth + 1);
            }
        }
    }
}

fn render_expr(e: &Expr, out: &mut String, depth: usize) {
    pad(out, depth);
    match &e.kind {
        ExprKind::IntLit(n) => out.push_str(&format!("int {n}\n")),
        ExprKind::FloatLit(f) => out.push_str(&format!("float {f}\n")),
        ExprKind::BoolLit(b) => out.push_str(&format!("bool {b}\n")),
        ExprKind::CharLit(c) => out.push_str(&format!("char {c:?}\n")),
        ExprKind::StrLit(s) => out.push_str(&format!("str {s:?}\n")),
        ExprKind::UnitLit => out.push_str("unit\n"),
        ExprKind::Path(segs) => out.push_str(&format!(
            "path {}\n",
            segs.iter()
                .map(|s| s.name.clone())
                .collect::<Vec<_>>()
                .join("::")
        )),
        ExprKind::Unary { op, expr } => {
            out.push_str(&format!("unary {op:?}\n"));
            render_expr(expr, out, depth + 1);
        }
        ExprKind::Binary { op, lhs, rhs } => {
            out.push_str(&format!("binary {op:?}\n"));
            render_expr(lhs, out, depth + 1);
            render_expr(rhs, out, depth + 1);
        }
        ExprKind::Assign { target, value } => {
            out.push_str("assign\n");
            render_expr(target, out, depth + 1);
            render_expr(value, out, depth + 1);
        }
        ExprKind::Call { callee, args } => {
            out.push_str("call\n");
            render_expr(callee, out, depth + 1);
            for a in args {
                render_expr(a, out, depth + 1);
            }
        }
        ExprKind::MethodCall { recv, name, args } => {
            out.push_str(&format!("method .{}\n", name.name));
            render_expr(recv, out, depth + 1);
            for a in args {
                render_expr(a, out, depth + 1);
            }
        }
        ExprKind::Field { obj, name } => {
            out.push_str(&format!("field .{}\n", name.name));
            render_expr(obj, out, depth + 1);
        }
        ExprKind::Index { obj, idx } => {
            out.push_str("index\n");
            render_expr(obj, out, depth + 1);
            render_expr(idx, out, depth + 1);
        }
        ExprKind::StructLit { path, fields } => {
            out.push_str(&format!(
                "structlit {}\n",
                path.iter()
                    .map(|s| s.name.clone())
                    .collect::<Vec<_>>()
                    .join("::")
            ));
            for (n, v) in fields {
                pad(out, depth + 1);
                out.push_str(&format!(".{} =\n", n.name));
                render_expr(v, out, depth + 2);
            }
        }
        ExprKind::ListLit(items) => {
            out.push_str("list\n");
            for i in items {
                render_expr(i, out, depth + 1);
            }
        }
        ExprKind::MapLit(entries) => {
            out.push_str("map\n");
            for (k, v) in entries {
                render_expr(k, out, depth + 1);
                render_expr(v, out, depth + 1);
            }
        }
        ExprKind::If { cond, then, else_ } => {
            out.push_str("if\n");
            render_expr(cond, out, depth + 1);
            render_block(then, out, depth + 1);
            if let Some(e) = else_ {
                pad(out, depth);
                out.push_str("else\n");
                render_expr(e, out, depth + 1);
            }
        }
        ExprKind::IfLet {
            pat,
            scrutinee,
            then,
            else_,
        } => {
            out.push_str(&format!("if-let {}\n", render_pat(pat)));
            render_expr(scrutinee, out, depth + 1);
            render_block(then, out, depth + 1);
            if let Some(e) = else_ {
                render_expr(e, out, depth + 1);
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            out.push_str("match\n");
            render_expr(scrutinee, out, depth + 1);
            for arm in arms {
                pad(out, depth + 1);
                out.push_str(&format!(
                    "arm {}{}\n",
                    render_pat(&arm.pat),
                    if arm.guard.is_some() {
                        " if <guard>"
                    } else {
                        ""
                    }
                ));
                render_expr(&arm.body, out, depth + 2);
            }
        }
        ExprKind::While { cond, body } => {
            out.push_str("while\n");
            render_expr(cond, out, depth + 1);
            render_block(body, out, depth + 1);
        }
        ExprKind::Loop { body } => {
            out.push_str("loop\n");
            render_block(body, out, depth + 1);
        }
        ExprKind::For { var, iter, body } => {
            out.push_str(&format!("for {}\n", var.name));
            render_expr(iter, out, depth + 1);
            render_block(body, out, depth + 1);
        }
        ExprKind::Range { lo, hi, inclusive } => {
            out.push_str(&format!("range{}\n", if *inclusive { "=" } else { "" }));
            render_expr(lo, out, depth + 1);
            render_expr(hi, out, depth + 1);
        }
        ExprKind::Break => out.push_str("break\n"),
        ExprKind::Continue => out.push_str("continue\n"),
        ExprKind::Return(v) => {
            out.push_str("return\n");
            if let Some(v) = v {
                render_expr(v, out, depth + 1);
            }
        }
        ExprKind::Block(b) => {
            out.push_str("block\n");
            render_block(b, out, depth + 1);
        }
        ExprKind::Closure { params, body, .. } => {
            out.push_str(&format!(
                "closure |{}|\n",
                params
                    .iter()
                    .map(|(n, t)| format!("{}:{}", n.name, render_ty(t.as_ref())))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            render_expr(body, out, depth + 1);
        }
        ExprKind::Try(inner) => {
            out.push_str("try?\n");
            render_expr(inner, out, depth + 1);
        }
        ExprKind::Error => out.push_str("<error>\n"),
    }
}

fn render_ty(t: Option<&TypeExpr>) -> String {
    let Some(t) = t else { return "_".into() };
    match &t.kind {
        TypeExprKind::Name(n) => n.name.clone(),
        TypeExprKind::App(n, args) => format!(
            "{}[{}]",
            n.name,
            args.iter()
                .map(|a| render_ty(Some(a)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        TypeExprKind::Fn(params, ret) => format!(
            "fn({}) -> {}",
            params
                .iter()
                .map(|p| render_ty(Some(p)))
                .collect::<Vec<_>>()
                .join(", "),
            ret.as_ref()
                .map(|r| render_ty(Some(r)))
                .unwrap_or("_".into())
        ),
        TypeExprKind::Dyn(n) => format!("dyn {}", n.name),
        TypeExprKind::Unit => "unit".into(),
        TypeExprKind::Error => "<error>".into(),
    }
}

fn render_pat(p: &Pattern) -> String {
    match &p.kind {
        PatternKind::Wildcard => "_".into(),
        PatternKind::Binding(n) => n.name.clone(),
        PatternKind::IntLit(n) => n.to_string(),
        PatternKind::BoolLit(b) => b.to_string(),
        PatternKind::CharLit(c) => format!("{c:?}"),
        PatternKind::StrLit(s) => format!("{s:?}"),
        PatternKind::Variant { path, args } => {
            let p: Vec<String> = path.iter().map(|s| s.name.clone()).collect();
            match args {
                VariantPatArgs::Unit => p.join("::"),
                VariantPatArgs::Tuple(pats) => format!(
                    "{}({})",
                    p.join("::"),
                    pats.iter().map(render_pat).collect::<Vec<_>>().join(", ")
                ),
                VariantPatArgs::Struct { fields, has_rest } => format!(
                    "{} {{ {}{} }}",
                    p.join("::"),
                    fields
                        .iter()
                        .map(|(n, sub)| format!("{}: {}", n.name, render_pat(sub)))
                        .collect::<Vec<_>>()
                        .join(", "),
                    if *has_rest { ", .." } else { "" }
                ),
            }
        }
        PatternKind::Struct {
            path,
            fields,
            has_rest,
        } => format!(
            "{} {{ {}{} }}",
            path.iter()
                .map(|s| s.name.clone())
                .collect::<Vec<_>>()
                .join("::"),
            fields
                .iter()
                .map(|(n, sub)| format!("{}: {}", n.name, render_pat(sub)))
                .collect::<Vec<_>>()
                .join(", "),
            if *has_rest { ", .." } else { "" }
        ),
        PatternKind::Or(alts) => alts.iter().map(render_pat).collect::<Vec<_>>().join(" | "),
        PatternKind::Error => "<error>".into(),
    }
}

fn golden(src: &str, expected: &str) {
    let parsed = wisp_compiler::parse(src);
    assert!(
        parsed.diags.is_empty(),
        "unexpected diags: {:?}",
        parsed.diags
    );
    let actual = render(&parsed.file);
    assert_eq!(
        actual.trim(),
        expected.trim(),
        "\n--- source ---\n{src}\n--- actual ---\n{actual}"
    );
}

#[test]
fn golden_functions_and_exprs() {
    golden(
        "fn add(a: int, b: int) -> int {\n    let c = a + b * 2\n    c\n}",
        "\
fn add(a:int, b:int) -> int
  let c:_ =
    binary Add
      path a
      binary Mul
        path b
        int 2
  expr
    path c",
    );
}

#[test]
fn golden_newline_continuations() {
    golden(
        "fn main() {\n    let s = \"x\"\n        .to_upper()\n    if s == \"X\" {\n        println(s)\n    }\n    else {\n        println(\"no\")\n    }\n}",
        "\
fn main() -> _
  let s:_ =
    method .to_upper
      str \"x\"
  expr
    if
      binary Eq
        path s
        str \"X\"
      expr
        call
          path println
          path s
    else
      block
        expr
          call
            path println
            str \"no\"",
    );
}

#[test]
fn golden_data_and_match() {
    golden(
        "enum E { A, B(int), C { v: bool } }\n\nfn f(e: E) -> int {\n    match e {\n        E::A => 0,\n        E::B(n) | E::B(_) => n,\n        E::C { v } if v => 1,\n        _ => 2,\n    }\n}",
        "\
enum E
  A
  B(int)
  C { v: bool }
fn f(e:E) -> int
  expr
    match
      path e
      arm E::A
        int 0
      arm E::B(n) | E::B(_)
        path n
      arm E::C { v: v } if <guard>
        int 1
      arm _
        int 2",
    );
}

#[test]
fn golden_interface_forms() {
    golden(
        "#[opaque]\nstruct Pane {}\n\nimpl Pane {\n    fn title(self) -> string\n}\n\nmod term {\n    const MAX: int\n    fn clear()\n}",
        "\
struct Pane [] opaque
impl Pane
  fn title(..)
mod term
  const MAX: int
  fn clear() -> _ <decl>",
    );
}

#[test]
fn golden_recovery_keeps_partial_ast() {
    // Broken input still yields a partial AST + diagnostics (PRD §5.1).
    let parsed = wisp_compiler::parse("fn ok() -> int { 1 }\nstruct Good { v: int }\nfn broken( {");
    assert!(!parsed.diags.is_empty());
    let mut names: Vec<String> = Vec::new();
    for item in &parsed.file.items {
        match item {
            Item::Fn(f) => names.push(f.name.name.clone()),
            Item::Struct(st) => names.push(st.name.name.clone()),
            _ => {}
        }
    }
    assert!(names.contains(&"ok".to_string()), "{names:?}");
    assert!(names.contains(&"Good".to_string()), "{names:?}");
}
