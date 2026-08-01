//! `fs` — file I/O (PRD §7). **Capability**: registering this module
//! grants scripts filesystem access.

use std::path::Path;

use wscript_core::Module;

fn err_str(e: std::io::Error) -> String {
    e.to_string()
}

pub fn fs() -> Module {
    let mut m = Module::new("fs");
    m.doc("File I/O (capability: filesystem access)");

    m.fn_("read_to_string", |path: &str| -> Result<String, String> {
        std::fs::read_to_string(path).map_err(err_str)
    });
    m.fn_("read_bytes", |path: &str| -> Result<Vec<i64>, String> {
        std::fs::read(path)
            .map(|bytes| bytes.into_iter().map(|b| b as i64).collect())
            .map_err(err_str)
    });
    m.fn_named(
        "write",
        ["path", "content"],
        |path: &str, content: &str| -> Result<(), String> {
            std::fs::write(path, content).map_err(err_str)
        },
    );
    m.fn_named(
        "append",
        ["path", "content"],
        |path: &str, content: &str| -> Result<(), String> {
            use std::io::Write;
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .and_then(|mut f| f.write_all(content.as_bytes()))
                .map_err(err_str)
        },
    );
    m.fn_("exists", |path: &str| Path::new(path).exists());
    m.fn_("is_file", |path: &str| Path::new(path).is_file());
    m.fn_("is_dir", |path: &str| Path::new(path).is_dir());
    m.fn_("list_dir", |path: &str| -> Result<Vec<String>, String> {
        let mut entries: Vec<String> = std::fs::read_dir(path)
            .map_err(err_str)?
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        entries.sort();
        Ok(entries)
    });
    m.fn_("create_dir", |path: &str| -> Result<(), String> {
        std::fs::create_dir(path).map_err(err_str)
    });
    m.fn_("create_dir_all", |path: &str| -> Result<(), String> {
        std::fs::create_dir_all(path).map_err(err_str)
    });
    m.fn_named(
        "copy",
        ["from", "to"],
        |from: &str, to: &str| -> Result<(), String> {
            std::fs::copy(from, to).map(|_| ()).map_err(err_str)
        },
    );
    m.fn_named(
        "rename",
        ["from", "to"],
        |from: &str, to: &str| -> Result<(), String> { std::fs::rename(from, to).map_err(err_str) },
    );
    m.fn_("remove_file", |path: &str| -> Result<(), String> {
        std::fs::remove_file(path).map_err(err_str)
    });
    m.fn_("remove_dir", |path: &str| -> Result<(), String> {
        std::fs::remove_dir(path).map_err(err_str)
    });

    // metadata
    m.fn_("size", |path: &str| -> Result<i64, String> {
        std::fs::metadata(path)
            .map(|m| m.len() as i64)
            .map_err(err_str)
    });
    m.fn_("mtime_unix", |path: &str| -> Result<f64, String> {
        let mtime = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .map_err(err_str)?;
        Ok(match mtime.duration_since(std::time::UNIX_EPOCH) {
            Ok(d) => d.as_secs_f64(),
            Err(e) => -e.duration().as_secs_f64(), // pre-1970 mtimes
        })
    });
    m.fn_("walk", |dir: &str| -> Result<Vec<String>, String> {
        // Recursive listing: every entry (files and directories) under
        // `dir`, as paths joined onto the argument, sorted. Iterative with
        // an explicit stack; unreadable subdirectories propagate as Err,
        // consistent with list_dir.
        let mut out = Vec::new();
        let mut stack = vec![std::path::PathBuf::from(dir)];
        while let Some(d) = stack.pop() {
            for entry in std::fs::read_dir(&d).map_err(err_str)? {
                let entry = entry.map_err(err_str)?;
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path.clone());
                }
                out.push(path.to_string_lossy().into_owned());
            }
        }
        out.sort();
        Ok(out)
    });

    // path helpers (pure string manipulation)
    m.fn_named("join", ["a", "b"], |a: &str, b: &str| {
        Path::new(a).join(b).to_string_lossy().into_owned()
    });
    m.fn_("parent", |path: &str| -> Option<String> {
        Path::new(path)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
    });
    m.fn_("file_name", |path: &str| -> Option<String> {
        Path::new(path)
            .file_name()
            .map(|p| p.to_string_lossy().into_owned())
    });
    m.fn_("ext", |path: &str| -> Option<String> {
        Path::new(path)
            .extension()
            .map(|p| p.to_string_lossy().into_owned())
    });
    m
}
