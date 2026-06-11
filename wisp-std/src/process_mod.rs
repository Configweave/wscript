//! `process` — command execution & environment (PRD §7). **Capability**:
//! registering this module grants scripts process control.

use wisp_core::Module;
use wisp_macros::Script;

/// Result of `process::run` (PRD §7).
#[derive(Script, Debug, Clone, PartialEq)]
#[script(crate_path = "wisp_core")]
pub struct Output {
    pub status: i64,
    pub stdout: String,
    pub stderr: String,
}

pub fn process() -> Module {
    process_with_args(Vec::new())
}

/// `script_args` is what `process::args()` returns — the CLI passes the
/// arguments after the script path (PRD §8).
pub fn process_with_args(script_args: Vec<String>) -> Module {
    let mut m = Module::new("process");
    m.doc("Command execution and environment (capability: process control)");
    m.ty::<Output>();

    m.doc_next("Run a command to completion; Err on spawn failure");
    m.fn_(
        "run",
        |cmd: &str, args: Vec<String>| -> Result<Output, String> {
            let out = std::process::Command::new(cmd)
                .args(&args)
                .output()
                .map_err(|e| e.to_string())?;
            Ok(Output {
                status: out.status.code().unwrap_or(-1) as i64,
                stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            })
        },
    );
    m.fn_("env", |key: &str| -> Option<String> { std::env::var(key).ok() });
    m.fn_("set_env", |key: &str, value: &str| {
        // SAFETY contract documented: single-threaded VM (PRD §4.3); the
        // host remains responsible for not racing the environment.
        unsafe { std::env::set_var(key, value) }
    });
    m.fn_("cwd", || -> Result<String, String> {
        std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .map_err(|e| e.to_string())
    });
    m.doc_next("Arguments passed after the script path on the command line");
    m.fn_("args", move || script_args.clone());
    m
}
