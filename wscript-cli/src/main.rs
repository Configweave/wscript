//! The `wscript` binary. Argv parsing over `wscript_cli`; everything else
//! lives in the library so it can be tested in-process.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    wscript_cli::run(&args)
}
