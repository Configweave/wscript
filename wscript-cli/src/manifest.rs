//! `wscript.toml` — the small project manifest (PRD §9.1). It lists `.wscripti`
//! interface files (paths relative to the manifest); `wscript check` and the
//! LSP honor it so scripts typecheck against host APIs.
//!
//! ```toml
//! # wscript.toml
//! interfaces = ["api.wscripti"]
//! ```

use std::path::{Path, PathBuf};

use wscript_compiler::wscripti::WscriptiIndex;

pub struct Manifest {
    pub interfaces: Vec<PathBuf>,
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
    let value: toml::Value = match text.parse() {
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
    Some(Manifest { interfaces })
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
