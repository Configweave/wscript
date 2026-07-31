//! `wscript.toml` handling — 105 lines that had no tests and no fixture
//! anywhere in the repo before the crate gained a lib target.
//!
//! The fixture under `tests/fixtures/project/` is the first `wscript.toml`
//! this repo has ever had, so it is also what exercises `Mode::Check`'s
//! disjoint registry (ADR-0002) and the `src_roots` path the LSP was
//! silently dropping.

use std::path::{Path, PathBuf};

use wscript_cli::manifest;

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/project")
        .join(rel)
}

#[test]
fn finds_the_manifest_beside_the_entry_file() {
    let m = manifest::find(&fixture("main.wscript")).expect("manifest found");
    assert_eq!(m.interfaces.len(), 1);
    assert!(m.interfaces[0].ends_with("api.wscripti"));
    assert_eq!(m.src_roots.len(), 1);
    assert!(m.src_roots[0].ends_with("scripts"));
}

/// The walk-up is the whole point: a script nested two directories deep
/// must still find the project's manifest.
#[test]
fn walks_up_from_a_nested_file() {
    let m = manifest::find(&fixture("nested/deep/inner.wscript")).expect("manifest found");
    assert_eq!(m.interfaces.len(), 1);
}

#[test]
fn paths_resolve_relative_to_the_manifest_not_the_entry() {
    let m = manifest::find(&fixture("nested/deep/inner.wscript")).expect("manifest found");
    assert!(
        m.interfaces[0].is_file(),
        "interface path {} should resolve to a real file",
        m.interfaces[0].display()
    );
    assert!(m.src_roots[0].is_dir());
}

#[test]
fn no_manifest_above_a_temp_dir() {
    // `/` is guaranteed to have no wscript.toml above it.
    assert!(manifest::find(Path::new("/")).is_none());
}

#[test]
fn interfaces_register_host_functions() {
    let m = manifest::find(&fixture("main.wscript")).expect("manifest found");
    let mut reg = wscript::Registry::new();
    let before = reg.modules.len();
    let indexes = manifest::load_interfaces(&m, &mut reg);

    assert_eq!(indexes.len(), 1, "one interface loaded");
    assert_eq!(reg.modules.len(), before + 1, "`host` module registered");
    let host = reg
        .modules
        .iter()
        .find(|m| m.name == "host")
        .expect("`host` module");
    assert!(
        host.fns.iter().any(|(name, ..)| name == "greet"),
        "greet registered"
    );
}

/// `wscript check` against a manifest resolves host calls through the
/// interface — with the CLI's stdlib deliberately absent (ADR-0002).
#[test]
fn check_resolves_against_the_interface() {
    let path = fixture("main.wscript");
    let code = wscript_cli::cmd_check(&path.to_string_lossy());
    assert_eq!(
        format!("{code:?}"),
        format!("{:?}", std::process::ExitCode::SUCCESS),
        "main.wscript should typecheck against api.wscripti + src_roots"
    );
}

/// `src_roots` is what makes `use helper` resolve from a sibling
/// directory. The LSP was constructing a resolver without it, so the
/// editor reported E0200 on imports the CLI resolved (fixed in #13).
#[test]
fn file_resolver_picks_up_src_roots() {
    let resolver = wscript_cli::file_resolver(&fixture("main.wscript").to_string_lossy());
    assert_eq!(resolver.roots.len(), 1);
    assert!(resolver.roots[0].ends_with("scripts"));
}
