use crate::types::{FnSig, Type};

/// Index into a [`DefTable`]. One id space covers structs, enums and traits;
/// `Type::Named` must point at a struct/enum entry, `Type::Dyn` at a trait.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DefId(pub u32);

impl DefId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// Reserved ids, present in every table (see [`DefTable::with_builtins`]).
pub const DEF_OPTION: DefId = DefId(0);
pub const DEF_RESULT: DefId = DefId(1);
pub const TRAIT_ADD: DefId = DefId(2);
pub const TRAIT_SUB: DefId = DefId(3);
pub const TRAIT_MUL: DefId = DefId(4);
pub const TRAIT_DIV: DefId = DefId(5);
pub const TRAIT_REM: DefId = DefId(6);
pub const TRAIT_NEG: DefId = DefId(7);
pub const TRAIT_EQ: DefId = DefId(8);
pub const TRAIT_ORD: DefId = DefId(9);
pub const TRAIT_DISPLAY: DefId = DefId(10);
pub const TRAIT_INDEX: DefId = DefId(11);
pub const FIRST_FREE_DEF: u32 = 12;

/// Tag values for the builtin enums (fixed; the VM and `?` lowering rely on
/// them).
pub const TAG_NONE: u32 = 0;
pub const TAG_SOME: u32 = 1;
pub const TAG_OK: u32 = 0;
pub const TAG_ERR: u32 = 1;

#[derive(Debug, Clone)]
pub enum DefKind {
    Struct(StructDef),
    Enum(EnumDef),
    Trait(TraitDef),
}

#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<(String, Type)>,
    /// Host handle type (`#[script(opaque)]`): no field access, methods only.
    pub opaque: bool,
    /// Registered by the host rather than declared in script.
    pub host: bool,
}

#[derive(Debug, Clone)]
pub struct EnumDef {
    pub name: String,
    pub variants: Vec<VariantDef>,
    pub host: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariantKind {
    Unit,
    Tuple,
    Struct,
}

#[derive(Debug, Clone)]
pub struct VariantDef {
    pub name: String,
    pub kind: VariantKind,
    /// Payload fields. Tuple variants have `"0"`, `"1"`… as names.
    pub fields: Vec<(String, Type)>,
}

#[derive(Debug, Clone)]
pub struct TraitDef {
    pub name: String,
    /// Method name → signature (excluding the implicit `self` receiver).
    pub methods: Vec<(String, FnSig)>,
    /// One of the builtin operator traits (Add … Index). Their method
    /// signatures are shape-checked specially (`Self` is `Type::Param(0)`).
    pub operator: bool,
}

/// Table of all nominal type and trait definitions visible to a compilation:
/// builtins, host-registered defs, then script defs appended in order.
#[derive(Debug, Clone)]
pub struct DefTable {
    pub defs: Vec<DefKind>,
}

impl Default for DefTable {
    fn default() -> Self {
        DefTable::with_builtins()
    }
}

impl DefTable {
    /// A table pre-seeded with `Option`, `Result` and the operator traits at
    /// their reserved ids.
    pub fn with_builtins() -> DefTable {
        let p0 = || Type::Param(0);
        let p1 = || Type::Param(1);
        let mut defs = Vec::with_capacity(FIRST_FREE_DEF as usize);
        defs.push(DefKind::Enum(EnumDef {
            name: "Option".into(),
            variants: vec![
                VariantDef {
                    name: "None".into(),
                    kind: VariantKind::Unit,
                    fields: vec![],
                },
                VariantDef {
                    name: "Some".into(),
                    kind: VariantKind::Tuple,
                    fields: vec![("0".into(), p0())],
                },
            ],
            host: false,
        }));
        defs.push(DefKind::Enum(EnumDef {
            name: "Result".into(),
            variants: vec![
                VariantDef {
                    name: "Ok".into(),
                    kind: VariantKind::Tuple,
                    fields: vec![("0".into(), p0())],
                },
                VariantDef {
                    name: "Err".into(),
                    kind: VariantKind::Tuple,
                    fields: vec![("0".into(), p1())],
                },
            ],
            host: false,
        }));
        // Operator traits (PRD §3.7). `Self` is Param(0) in these signatures;
        // impls provide concrete types which the checker shape-checks.
        let binop = |name: &str, method: &str| {
            DefKind::Trait(TraitDef {
                name: name.into(),
                methods: vec![(method.into(), FnSig::new(vec![p0()], p0()))],
                operator: true,
            })
        };
        defs.push(binop("Add", "add"));
        defs.push(binop("Sub", "sub"));
        defs.push(binop("Mul", "mul"));
        defs.push(binop("Div", "div"));
        defs.push(binop("Rem", "rem"));
        defs.push(DefKind::Trait(TraitDef {
            name: "Neg".into(),
            methods: vec![("neg".into(), FnSig::new(vec![], p0()))],
            operator: true,
        }));
        defs.push(DefKind::Trait(TraitDef {
            name: "Eq".into(),
            methods: vec![("eq".into(), FnSig::new(vec![p0()], Type::Bool))],
            operator: true,
        }));
        defs.push(DefKind::Trait(TraitDef {
            name: "Ord".into(),
            // cmp returns -1 / 0 / 1
            methods: vec![("cmp".into(), FnSig::new(vec![p0()], Type::Int))],
            operator: true,
        }));
        defs.push(DefKind::Trait(TraitDef {
            name: "Display".into(),
            methods: vec![("fmt".into(), FnSig::new(vec![], Type::Str))],
            operator: true,
        }));
        defs.push(DefKind::Trait(TraitDef {
            name: "Index".into(),
            // Impls declare their own concrete index/output types; Param(1)
            // here only records the two-slot shape.
            methods: vec![("index".into(), FnSig::new(vec![p1()], p0()))],
            operator: true,
        }));
        debug_assert_eq!(defs.len(), FIRST_FREE_DEF as usize);
        DefTable { defs }
    }

    pub fn push(&mut self, def: DefKind) -> DefId {
        let id = DefId(self.defs.len() as u32);
        self.defs.push(def);
        id
    }

    pub fn get(&self, id: DefId) -> &DefKind {
        &self.defs[id.index()]
    }

    pub fn name_of(&self, id: DefId) -> &str {
        match &self.defs[id.index()] {
            DefKind::Struct(s) => &s.name,
            DefKind::Enum(e) => &e.name,
            DefKind::Trait(t) => &t.name,
        }
    }

    pub fn trait_name(&self, id: DefId) -> &str {
        self.name_of(id)
    }

    pub fn as_struct(&self, id: DefId) -> Option<&StructDef> {
        match &self.defs[id.index()] {
            DefKind::Struct(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_enum(&self, id: DefId) -> Option<&EnumDef> {
        match &self.defs[id.index()] {
            DefKind::Enum(e) => Some(e),
            _ => None,
        }
    }

    pub fn as_trait(&self, id: DefId) -> Option<&TraitDef> {
        match &self.defs[id.index()] {
            DefKind::Trait(t) => Some(t),
            _ => None,
        }
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }
}
