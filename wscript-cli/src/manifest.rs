//! `wscript.toml` — the small project manifest (PRD §9.1). It lists `.wscripti`
//! interface files (paths relative to the manifest); `wscript check` and the
//! LSP honor it so scripts typecheck against host APIs.
//!
//! ```toml
//! # wscript.toml
//! interfaces = ["api.wscripti"]
//! src_roots = ["scripts"]        # `use name` script-import search dirs
//! ```

use std::path::{Path, PathBuf};

use wscript::Session;
use wscript_compiler::wscripti::WscriptiIndex;

#[derive(Clone)]
pub struct Manifest {
    pub interfaces: Vec<PathBuf>,
    /// Directories searched (after the importing file's own directory)
    /// when resolving `use name` script imports.
    pub src_roots: Vec<PathBuf>,
}

/// Walk up from `start` looking for `wscript.toml`.
pub fn find(start: &Path) -> Option<Manifest> {
    let mut dir = if start.is_dir() {
        start.to_path_buf()
    } else {
        start.parent()?.to_path_buf()
    };
    if dir.as_os_str().is_empty() {
        dir = PathBuf::from(".");
    }
    let mut dir = dir.canonicalize().ok()?;
    loop {
        let candidate = dir.join("wscript.toml");
        if candidate.is_file() {
            return parse(&candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn parse(path: &Path) -> Option<Manifest> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: toml::Value = match toml::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("warning: {} is not valid TOML: {e}", path.display());
            return None;
        }
    };
    let dir = path.parent()?.to_path_buf();
    let interfaces = value
        .get("interfaces")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| dir.join(s))
                .collect()
        })
        .unwrap_or_default();
    let src_roots = value
        .get("src_roots")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| dir.join(s))
                .collect()
        })
        .unwrap_or_default();
    Some(Manifest {
        interfaces,
        src_roots,
    })
}

/// What a [`Session`] is being built *for*.
///
/// The two modes differ in exactly one deliberate way: which host the
/// script is checked against. Running means the CLI's own stdlib is the
/// host, so that is the registry. Checking means the *declared* host is,
/// so a manifest's `.wscripti` interfaces replace the stdlib rather than
/// layering over it — otherwise a same-named CLI module would shadow the
/// embedder's and mis-check real scripts (ADR-0002).
///
/// Both modes take `src_roots` from the manifest. That is not a
/// difference, and making it a mode-independent part of building the
/// session is what stops the language server from silently dropping it.
pub enum Mode {
    /// `wscript run` / `wscript repl`: the stdlib is the host, and the
    /// script sees these arguments through `process::args`.
    Run { script_args: Vec<String> },
    /// `wscript check` / `wscript lsp`: compile only, against the
    /// manifest's declared interfaces where there is one.
    Check,
}

/// A wscript project as the CLI sees it: a [`Session`] to compile with,
/// plus the `.wscripti` indexes that let the language server jump to a
/// host declaration.
///
/// Cheap to clone — the session is two `Arc`s and the indexes are a
/// handful of span tables.
#[derive(Clone)]
pub struct Project {
    pub session: Session,
    pub interfaces: Vec<(PathBuf, WscriptiIndex)>,
}

/// Build the project containing `entry`, in `mode`.
///
/// This is the one place that turns "a path on disk" into "a configured
/// compiler". Every command goes through it, which is what keeps the
/// editor and the CLI resolving the same imports: finding the manifest,
/// choosing the registry and wiring `src_roots` happen together or not
/// at all.
///
/// With no `wscript.toml` above `entry`, both modes fall back to the
/// stdlib and filesystem-relative imports — the behaviour of a bare
/// `wscript run foo.wscript`.
pub fn project_for(entry: &Path, mode: Mode) -> Project {
    let manifest = find(entry);
    let src_roots = manifest
        .as_ref()
        .map(|m| m.src_roots.clone())
        .unwrap_or_default();
    let builder = Session::builder().src_roots(src_roots);
    match (mode, manifest) {
        (Mode::Check, Some(m)) => {
            let mut registry = wscript::Registry::new();
            let interfaces = load_interfaces(&m, &mut registry);
            Project {
                session: builder.registry(registry).build(),
                interfaces,
            }
        }
        (mode, _) => {
            let script_args = match mode {
                Mode::Run { script_args } => script_args,
                Mode::Check => Vec::new(),
            };
            Project {
                session: builder
                    .modules(wscript_std::all_modules(script_args))
                    .build(),
                interfaces: Vec::new(),
            }
        }
    }
}

/// Load every interface listed by the manifest into the registry (live
/// registrations win on name clashes). Returns the combined definition
/// index (file path per entry for goto-definition).
pub fn load_interfaces(
    manifest: &Manifest,
    reg: &mut wscript::Registry,
) -> Vec<(PathBuf, WscriptiIndex)> {
    let mut indexes = Vec::new();
    for path in &manifest.interfaces {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let (diags, index) = wscript_compiler::wscripti::load(&text, reg);
                for d in diags {
                    eprintln!("warning: {}: [{}] {}", path.display(), d.code, d.message);
                }
                indexes.push((path.clone(), index));
            }
            Err(e) => {
                eprintln!("warning: cannot read interface {}: {e}", path.display());
            }
        }
    }
    indexes
}
