//! Generates one `#[test]` per script under `tests/scripts/`, so a failure
//! names the script and `cargo test <name>` selects it.
//!
//! Globbing rather than a hand-written list: a new script is then covered
//! by adding the file, with no registration step to forget. That failure
//! mode is not hypothetical — the LSP's builtin-method tables went 14
//! entries stale exactly because a change did not touch a second place.

use std::path::Path;

fn main() {
    let dir = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .parent()
        .expect("workspace root")
        .join("tests/scripts");
    println!("cargo:rerun-if-changed={}", dir.display());

    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "wscript"))
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .collect();
    names.sort();
    assert!(!names.is_empty(), "no scripts found in {}", dir.display());

    let mut out = String::new();
    for name in &names {
        assert!(
            name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !name.starts_with(|c: char| c.is_ascii_digit()),
            "script name `{name}` is not a valid Rust identifier"
        );
        out.push_str(&format!(
            "#[test]\nfn {name}() {{ check_script(\"{name}\"); }}\n\n"
        ));
    }

    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("script_tests.rs");
    std::fs::write(&dest, out).unwrap_or_else(|e| panic!("writing {}: {e}", dest.display()));
}
