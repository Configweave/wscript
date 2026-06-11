// process module.
// expect: out: hi
// expect: status: 0
// expect: has_path: true

use process

fn main() {
    match process::run("echo", ["hi"]) {
        Ok(out) => {
            println("out: " + out.stdout.trim())
            println(fmt("status: {}", out.status))
        }
        Err(e) => println("spawn error: " + e),
    }
    println(fmt("has_path: {}", process::env("PATH").is_some()))
}
