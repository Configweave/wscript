//! VM behaviour tests written *in wisp* (PRD §11): every `.wisp` file under
//! `tests/scripts/` (workspace root) is compiled and executed through the
//! real `wisp` binary; its stdout and exit code are asserted against
//! directives embedded in the script:
//!
//! ```text
//! // expect: <exact stdout line>     (repeatable, in order)
//! // exit: <code>                    (default 0)
//! // error: <substring of stderr>    (compile or runtime failure expected)
//! ```

use std::path::{Path, PathBuf};
use std::process::Command;

fn scripts_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests/scripts")
}

#[test]
fn run_all_scripts() {
    let dir = scripts_dir();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "wisp"))
        .collect();
    entries.sort();
    assert!(!entries.is_empty(), "no test scripts found");

    let mut failures = Vec::new();
    for path in entries {
        if let Err(msg) = run_one(&path) {
            failures.push(format!("{}: {msg}", path.file_name().unwrap().to_string_lossy()));
        }
    }
    if !failures.is_empty() {
        panic!("script test failures:\n{}", failures.join("\n"));
    }
}

fn run_one(path: &Path) -> Result<(), String> {
    let source = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut expected_lines = Vec::new();
    let mut expected_exit = 0i32;
    let mut expected_error: Option<String> = None;
    for line in source.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("// expect:") {
            expected_lines.push(rest.strip_prefix(' ').unwrap_or(rest).to_string());
        } else if let Some(rest) = line.strip_prefix("// exit:") {
            expected_exit = rest.trim().parse().map_err(|e| format!("bad exit: {e}"))?;
        } else if let Some(rest) = line.strip_prefix("// error:") {
            expected_error = Some(rest.trim().to_string());
        }
    }

    let output = Command::new(env!("CARGO_BIN_EXE_wisp"))
        .arg("run")
        .arg(path)
        .output()
        .map_err(|e| e.to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let code = output.status.code().unwrap_or(-1);

    if let Some(needle) = expected_error {
        if code == 0 {
            return Err(format!("expected failure containing `{needle}`, but exit was 0"));
        }
        if !stderr.contains(&needle) {
            return Err(format!(
                "stderr does not contain `{needle}`:\n--- stderr ---\n{stderr}"
            ));
        }
        return Ok(());
    }

    let actual_lines: Vec<&str> = stdout.lines().collect();
    let expected_refs: Vec<&str> = expected_lines.iter().map(String::as_str).collect();
    if actual_lines != expected_refs {
        return Err(format!(
            "stdout mismatch\n--- expected ---\n{}\n--- actual ---\n{}\n--- stderr ---\n{}",
            expected_refs.join("\n"),
            actual_lines.join("\n"),
            stderr
        ));
    }
    if code != expected_exit {
        return Err(format!(
            "exit code {code}, expected {expected_exit}\n--- stderr ---\n{stderr}"
        ));
    }
    Ok(())
}
