//! Every script we ship, verified: no register operand escapes its frame
//! and no jump leaves its function.
//!
//! The VM does not re-validate operands — a register index past `n_regs`
//! is a panic, not a fault (`wscript-vm` module docs). The emitter is the
//! only thing standing behind that, and its guarantee used to be a
//! high-water mark maintained by convention. [`wscript_core::verify`] is
//! the assertion; this suite is where it meets real bytecode.
//!
//! The corpus is the same one `scripts.rs` and `fuel_snapshot.rs` run
//! (globbed, so a new script is covered by adding the file), plus
//! `examples/`. Scripts that do not compile — the `_err` fixtures — have
//! no bytecode to verify and are skipped.

use std::path::{Path, PathBuf};

use wscript_cli::manifest::{Mode, project_for};
use wscript_core::verify::verify_report;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

/// Every `.wscript` directly under `dir`, sorted.
fn scripts_in(dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "wscript"))
        .collect();
    assert!(!paths.is_empty(), "no scripts under {}", dir.display());
    paths.sort();
    paths
}

/// Compile one script the way `wscript run` would, and verify what came
/// out. `None` when it does not compile (an `_err` fixture).
fn verify_script(path: &Path) -> Option<Result<(), String>> {
    let source = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let session = project_for(
        path,
        Mode::Run {
            script_args: Vec::new(),
        },
    )
    .session;
    let compiled = session.compile(&path.to_string_lossy(), &source).ok()?;
    Some(verify_report(&compiled.unit))
}

/// Verify every script under `dir`, reporting all failures at once: one
/// emitter path usually breaks a family of scripts, and the family names
/// the path.
fn verify_corpus(dir: &Path) {
    let mut verified = 0;
    let mut failures = Vec::new();
    for path in scripts_in(dir) {
        match verify_script(&path) {
            None => {}
            Some(Ok(())) => verified += 1,
            Some(Err(report)) => failures.push(format!("{}\n{report}", path.display())),
        }
    }
    assert!(
        failures.is_empty(),
        "{} script(s) emitted out-of-range bytecode:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    assert!(verified > 0, "nothing under {} compiled", dir.display());
}

#[test]
fn the_script_corpus_verifies() {
    verify_corpus(&workspace_root().join("tests/scripts"));
}

#[test]
fn the_examples_verify() {
    verify_corpus(&workspace_root().join("examples"));
}
