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

// ------------------------------------------------------------- projects

use manifest::{Mode, project_for};

fn analyze(mode: Mode) -> wscript::Analysis {
    let path = fixture("main.wscript");
    let source = std::fs::read_to_string(&path).expect("fixture readable");
    project_for(&path, mode)
        .session
        .analyze(&path.to_string_lossy(), &source)
}

fn errors(a: &wscript::Analysis) -> Vec<String> {
    a.parse
        .diags
        .iter()
        .chain(a.check.diags.iter())
        .filter(|d| d.severity == wscript::Severity::Error)
        .map(|d| format!("[{}] {}", d.code, d.message))
        .collect()
}

/// `src_roots` is what makes `use helper` resolve from a sibling
/// directory, and it must come out of the manifest in *both* modes. The
/// language server used to build its resolver separately, with no roots,
/// so the editor reported E0200 on imports `wscript check` resolved
/// happily — the bug #13 exists to kill.
#[test]
fn both_modes_resolve_src_roots_imports() {
    for mode in [
        Mode::Check,
        Mode::Run {
            script_args: vec![],
        },
    ] {
        let a = analyze(mode);
        // `use host` is a check-mode-only module (it comes from the
        // .wscripti), so run mode legitimately cannot find it — but
        // `helper` is a script file, and both modes must reach it.
        assert!(
            !errors(&a).iter().any(|e| e.contains("`helper`")),
            "`use helper` must resolve through src_roots: {:?}",
            errors(&a)
        );
        assert_eq!(a.sources.len(), 2, "entry plus scripts/helper.wscript");
        assert!(a.sources[1].0.ends_with("helper.wscript"));
    }
}

/// The editor and the CLI must reach the same conclusion about the same
/// file. They start from different places — the language server builds
/// its project from the workspace *root* at `initialize`, `wscript check`
/// from the *entry file* — and that is precisely where they used to
/// diverge, because only one of them carried `src_roots` forward.
#[test]
fn the_editor_and_the_cli_agree() {
    let entry = fixture("main.wscript");
    let entry_str = entry.to_string_lossy().into_owned();
    let source = std::fs::read_to_string(&entry).expect("fixture readable");

    let editor = project_for(&fixture(""), Mode::Check).session;
    let cli = project_for(&entry, Mode::Check).session;

    let a = editor.analyze(&entry_str, &source);
    let b = cli.analyze(&entry_str, &source);

    let files = |x: &wscript::Analysis| -> Vec<String> {
        x.sources.iter().map(|(p, _)| p.clone()).collect()
    };
    assert_eq!(files(&a), files(&b), "same import set");
    assert_eq!(errors(&a), errors(&b), "same diagnostics");
    assert!(errors(&a).is_empty(), "and no errors: {:?}", errors(&a));
}

/// The entry file typechecks completely in check mode: imports from
/// `src_roots`, host calls from the `.wscripti`.
#[test]
fn check_mode_typechecks_the_fixture() {
    assert!(errors(&analyze(Mode::Check)).is_empty());
}

/// ADR-0002: with a manifest present, `Mode::Check`'s registry holds the
/// declared interfaces and nothing else, so a same-named CLI module
/// cannot shadow the embedder's.
#[test]
fn check_mode_registry_is_the_declared_host_only() {
    let project = project_for(&fixture("main.wscript"), Mode::Check);
    let names: Vec<&str> = project
        .session
        .registry()
        .modules
        .iter()
        .map(|m| m.name.as_str())
        .collect();
    assert_eq!(names, ["host"], "only the .wscripti module is registered");
    assert_eq!(project.interfaces.len(), 1, "indexed for goto-definition");
}

/// Run mode is the other half of ADR-0002: the stdlib *is* the host, and
/// the interfaces are not consulted at all.
#[test]
fn run_mode_registry_is_the_stdlib() {
    let project = project_for(
        &fixture("main.wscript"),
        Mode::Run {
            script_args: vec![],
        },
    );
    let names: Vec<&str> = project
        .session
        .registry()
        .modules
        .iter()
        .map(|m| m.name.as_str())
        .collect();
    assert!(names.contains(&"fs"), "stdlib registered: {names:?}");
    assert!(
        !names.contains(&"host"),
        "the .wscripti host is not a run-time module: {names:?}"
    );
    assert!(project.interfaces.is_empty());
}

/// Outside a project both modes fall back to the stdlib — a bare
/// `wscript run foo.wscript` still works.
#[test]
fn without_a_manifest_check_falls_back_to_the_stdlib() {
    let project = project_for(Path::new("/nonexistent/foo.wscript"), Mode::Check);
    assert!(
        project
            .session
            .registry()
            .modules
            .iter()
            .any(|m| m.name == "fs")
    );
    assert!(project.interfaces.is_empty());
}
