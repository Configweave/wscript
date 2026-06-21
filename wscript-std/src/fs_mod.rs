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
    m.fn_("write", |path: &str, content: &str| -> Result<(), String> {
        std::fs::write(path, content).map_err(err_str)
    });
    m.fn_(
        "append",
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
    m.fn_("copy", |from: &str, to: &str| -> Result<(), String> {
        std::fs::copy(from, to).map(|_| ()).map_err(err_str)
    });
    m.fn_("rename", |from: &str, to: &str| -> Result<(), String> {
        std::fs::rename(from, to).map_err(err_str)
    });
    m.fn_("remove_file", |path: &str| -> Result<(), String> {
        std::fs::remove_file(path).map_err(err_str)
    });
    m.fn_("remove_dir", |path: &str| -> Result<(), String> {
        std::fs::remove_dir(path).map_err(err_str)
    });

    // path helpers (pure string manipulation)
    m.fn_("join", |a: &str, b: &str| {
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
