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
        on_disk, expected,
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
