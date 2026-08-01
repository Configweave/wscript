//! Keep `wscript-std/wscripti/std.wscripti` in sync with the actual registrations
//! (PRD §9.1: wscript-std ships generated interface files), and prove the
//! interface parses with the same parser as scripts.
//!
//! Regenerate with: `WSCRIPT_REGEN_WSCRIPTI=1 cargo test -p wscript-cli --test wscripti_gen`

use std::path::PathBuf;

use wscript_core::bytecode::Const;

fn wscripti_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("wscript-std/wscripti/std.wscripti")
}

fn current_interface() -> String {
    let mut ctx = wscript::Context::new();
    for m in wscript_std::all_modules(Vec::new()) {
        ctx = ctx.module(m);
    }
    ctx.interface_text()
}

#[test]
fn std_wscripti_is_in_sync() {
    let expected = current_interface();
    let path = wscripti_path();
    if std::env::var("WSCRIPT_REGEN_WSCRIPTI").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &expected).unwrap();
        return;
    }
    let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        on_disk,
        expected,
        "stale {}; regenerate with WSCRIPT_REGEN_WSCRIPTI=1 cargo test -p wscript-cli --test wscripti_gen",
        path.display()
    );
}

#[test]
fn std_wscripti_parses_with_the_script_parser() {
    // PRD §9.1: the .wscripti grammar is a strict subset of the language
    // grammar — the same parser must accept it without diagnostics.
    let text = current_interface();
    let parsed = wscript_compiler::parse(&text);
    assert!(
        parsed.diags.is_empty(),
        "interface text does not parse cleanly: {:?}\n--- text ---\n{text}",
        parsed.diags
    );
}

/// Declared parameter names (issue #22) are written out and read back
/// unchanged; undeclared ones stay positional placeholders rather than
/// being laundered into names the host never gave.
#[test]
fn declared_parameter_names_round_trip() {
    let text = current_interface();
    assert!(
        text.contains("fn atan2(y: float, x: float)"),
        "declared names belong in the interface:\n{text}"
    );

    let mut reg = wscript::Registry::new();
    let (diags, _index) = wscript_compiler::wscripti::load(&text, &mut reg);
    assert!(diags.is_empty(), "{diags:?}");

    let math = reg.module("math").expect("math module");
    let atan2 = math.fns.iter().find(|f| f.name == "atan2").unwrap();
    assert_eq!(
        atan2.param_names(),
        Some(&["y".to_string(), "x".into()][..])
    );
    let sqrt = math.fns.iter().find(|f| f.name == "sqrt").unwrap();
    assert_eq!(sqrt.param_names(), None, "`sqrt` declares no names");

    // Re-emitting reproduces the file verbatim, consts included.
    let reloaded = wscript::Context::from_registry(reg).interface_text();
    assert_eq!(
        reloaded, text,
        "declarations must survive a load and re-emit unchanged"
    );
}

/// A module exercising every constant shape a host can register, including
/// the ones with no literal spelling (±inf, NaN) and the ones whose text
/// needs escaping (braces read as interpolation holes; quotes and control
/// characters need backslashes).
fn const_zoo() -> wscript::Module {
    let mut m = wscript::Module::new("zoo");
    m.const_("MAX_PANES", 16i64);
    m.const_("MIN", -9223372036854775808i64);
    m.const_("MAX", 9223372036854775807i64);
    m.const_("RATIO", 1.5f64);
    m.const_("NEG", -0.25f64);
    m.const_("TINY", 5e-324f64);
    m.const_("HUGE", f64::MAX);
    m.const_("INF", f64::INFINITY);
    m.const_("NEG_INF", f64::NEG_INFINITY);
    m.const_("NAN", f64::NAN);
    m.const_("ZERO", 0.0f64);
    m.const_("NEG_ZERO", -0.0f64);
    m.const_("ON", true);
    m.const_("OFF", false);
    m.const_("TICK", '\n');
    m.const_("QUOTE", '\'');
    m.const_("EMOJI", '\u{1F600}');
    m.const_("GREETING", "hi \"there\"\n\tbye\\");
    m.const_("TEMPLATE", "score: {x} and {{literal}}");
    m.const_("EMPTY", "");
    m
}

fn zoo_interface() -> String {
    wscript::Context::new().module(const_zoo()).interface_text()
}

/// The bug this file's ticket (#15) exists for: the emitter wrote const
/// values into a comment and the loader invented a zero, so `wscript check`
/// const-folded 0 where `wscript run` folded 16.
#[test]
fn check_and_run_agree_on_host_const_values() {
    let live = wscript::Context::new().module(const_zoo());
    let mut from_interface = wscript::Registry::new();
    let (diags, _index) = wscript_compiler::wscripti::load(&zoo_interface(), &mut from_interface);
    assert!(diags.is_empty(), "{diags:?}");

    let src = "use zoo\nfn main() -> int { zoo::MAX_PANES }";
    let checked = wscript_compiler::compile(src, &from_interface)
        .unwrap()
        .unit;
    let ran = live.compile(src).unwrap();

    // Module consts fold at compile time, so the interface-only unit runs
    // without any host implementation — and must fold the same value.
    let fold = |reg: &wscript::Registry, unit: &wscript::CompiledUnit| -> i64 {
        let mut vm = wscript_vm::Vm::new(reg, wscript_vm::VmConfig::default());
        match vm.call_name(unit, "main", vec![]).unwrap() {
            wscript::Value::Int(n) => n,
            other => panic!("expected an int, got {other:?}"),
        }
    };
    let from_check = fold(&from_interface, &checked);
    assert_eq!(from_check, 16, "the checker folds the registered value");
    assert_eq!(from_check, fold(live.registry(), &ran));
}

/// Every registered constant survives emit → parse → load unchanged.
#[test]
fn const_values_round_trip() {
    let text = zoo_interface();
    let mut reg = wscript::Registry::new();
    let (diags, _index) = wscript_compiler::wscripti::load(&text, &mut reg);
    assert!(diags.is_empty(), "{diags:?}\n--- text ---\n{text}");

    let original = const_zoo();
    let live = wscript::Context::new().module(original);
    let expected = &live.registry().module("zoo").unwrap().consts;
    let actual = &reg.module("zoo").expect("zoo module").consts;
    assert_eq!(actual.len(), expected.len(), "{text}");
    for ((n1, t1, c1), (n2, t2, c2)) in expected.iter().zip(actual) {
        assert_eq!(n1, n2);
        assert_eq!(t1, t2, "{n1}");
        assert!(
            same_const(c1, c2),
            "{n1}: {c1:?} loaded back as {c2:?}\n{text}"
        );
    }

    // Re-emitting is a fixed point: the second render is byte-identical.
    assert_eq!(wscript::Context::from_registry(reg).interface_text(), text);
}

/// `Const`'s `PartialEq` is no use for floats here: NaN is never equal to
/// itself, and the round trip must still be judged to have preserved it.
fn same_const(a: &Const, b: &Const) -> bool {
    use Const::Float;
    match (a, b) {
        (Float(x), Float(y)) => (x.is_nan() && y.is_nan()) || x.to_bits() == y.to_bits(),
        _ => a == b,
    }
}

/// A constant the loader cannot fold is reported, never quietly stood in
/// for: an invented value is exactly the `check`/`run` divergence #15 was
/// filed about.
#[test]
fn a_constant_without_a_usable_value_is_reported() {
    for (src, what) in [
        ("mod m {\n    const A: int\n}\n", "no value at all"),
        ("mod m {\n    const A: int = 1.5\n}\n", "float for an int"),
        ("mod m {\n    const A: float = 1\n}\n", "int for a float"),
        (
            "mod m {\n    const A: string = 'c'\n}\n",
            "char for a string",
        ),
        (
            "mod m {\n    const A: int = 1 / 0\n}\n",
            "int divide by zero",
        ),
        (
            "mod m {\n    const A: int = 9223372036854775807 + 1\n}\n",
            "int overflow",
        ),
        (
            "mod m {\n    const A: bool = other::B\n}\n",
            "not a literal",
        ),
    ] {
        let mut reg = wscript::Registry::new();
        let (diags, _index) = wscript_compiler::wscripti::load(src, &mut reg);
        // E0271 specifically: the interface loader rejected it, rather
        // than the parser having failed to reach it.
        assert!(
            diags.iter().any(|d| d.code == "E0271"),
            "{what} should be reported by the loader: {src}\n{diags:?}"
        );
    }
}

/// One `resolve_type`: an interface cannot declare a type no script could
/// write. Before #15 the loader had its own copy with none of these checks.
#[test]
fn interfaces_cannot_declare_types_a_script_could_not_write() {
    // The codes are the checker's own — the point is that these come from
    // the shared resolver and not from a second opinion in the loader.
    for (src, code) in [
        ("mod m {\n    fn f(a0: Map[float, int])\n}\n", "E0214"),
        ("mod m {\n    fn f(a0: weak[int])\n}\n", "E0213"),
        ("mod m {\n    fn f(a0: List[int, int])\n}\n", "E0210"),
        ("mod m {\n    fn f(a0: List)\n}\n", "E0210"),
        ("mod m {\n    fn f(a0: Nope)\n}\n", "E0212"),
    ] {
        let mut reg = wscript::Registry::new();
        let (diags, _index) = wscript_compiler::wscripti::load(src, &mut reg);
        assert!(
            diags.iter().any(|d| d.code == code),
            "{src} should be rejected with {code}\n{diags:?}"
        );
    }
}

#[test]
fn scripts_typecheck_against_wscripti_alone() {
    // PRD §9.1: the LSP/check can typecheck against interfaces with no
    // live host functions at all.
    let text = current_interface();
    let mut reg = wscript::Registry::new();
    let (diags, _index) = wscript_compiler::wscripti::load(&text, &mut reg);
    assert!(diags.is_empty(), "{diags:?}");

    let ok = wscript_compiler::compile(
        "use math\nuse json\nfn main() -> float { \
             let v = json::parse(\"1\").unwrap()\n \
             math::abs(v.as_float().unwrap_or(0.0)) }",
        &reg,
    );
    assert!(ok.is_ok());

    // Misuse is still a type error.
    let err = wscript_compiler::compile("use math\nfn main() { math::abs(\"x\"); }", &reg);
    assert!(err.is_err());

    // Running against stubs faults rather than panicking.
    let unit = wscript_compiler::compile("use math\nfn main() -> float { math::abs(-1.0) }", &reg)
        .unwrap()
        .unit;
    let mut vm = wscript_vm::Vm::new(&reg, wscript_vm::VmConfig::default());
    let result = vm.call_name(&unit, "main", vec![]);
    assert!(result.is_err());
}
