//! Keep `wisp-std/wispi/std.wispi` in sync with the actual registrations
//! (PRD §9.1: wisp-std ships generated interface files), and prove the
//! interface parses with the same parser as scripts.
//!
//! Regenerate with: `WISP_REGEN_WISPI=1 cargo test -p wisp-cli --test wispi_gen`

use std::path::PathBuf;

fn wispi_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("wisp-std/wispi/std.wispi")
}

fn current_interface() -> String {
    let mut ctx = wisp::Context::new();
    for m in wisp_std::all_modules(Vec::new()) {
        ctx = ctx.module(m);
    }
    ctx.interface_text()
}

#[test]
fn std_wispi_is_in_sync() {
    let expected = current_interface();
    let path = wispi_path();
    if std::env::var("WISP_REGEN_WISPI").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &expected).unwrap();
        return;
    }
    let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        on_disk,
        expected,
        "stale {}; regenerate with WISP_REGEN_WISPI=1 cargo test -p wisp-cli --test wispi_gen",
        path.display()
    );
}

#[test]
fn std_wispi_parses_with_the_script_parser() {
    // PRD §9.1: the .wispi grammar is a strict subset of the language
    // grammar — the same parser must accept it without diagnostics.
    let text = current_interface();
    let parsed = wisp_compiler::parse(&text);
    assert!(
        parsed.diags.is_empty(),
        "interface text does not parse cleanly: {:?}\n--- text ---\n{text}",
        parsed.diags
    );
}

#[test]
fn scripts_typecheck_against_wispi_alone() {
    // PRD §9.1: the LSP/check can typecheck against interfaces with no
    // live host functions at all.
    let text = current_interface();
    let mut reg = wisp::Registry::new();
    let (diags, _index) = wisp_compiler::wispi::load(&text, &mut reg);
    assert!(diags.is_empty(), "{diags:?}");

    let ok = wisp_compiler::compile(
        "use math\nuse json\nfn main() -> float { \
             let v = json::parse(\"1\").unwrap()\n \
             math::abs(v.as_float().unwrap_or(0.0)) }",
        &reg,
    );
    assert!(ok.is_ok());

    // Misuse is still a type error.
    let err = wisp_compiler::compile("use math\nfn main() { math::abs(\"x\"); }", &reg);
    assert!(err.is_err());

    // Running against stubs faults rather than panicking.
    let unit = wisp_compiler::compile("use math\nfn main() -> float { math::abs(-1.0) }", &reg)
        .unwrap()
        .unit;
    let mut vm = wisp_vm::Vm::new(&reg);
    let result = vm.call_name(&unit, "main", vec![]);
    assert!(result.is_err());
}
