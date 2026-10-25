// `expect` on an `Err` faults with its message and the `Err` payload, so
// the cause is not lost behind the caller's description of the step.
// error: install failed: exit 3: disk full

fn install() -> Result[int, string] {
    Err("exit 3: disk full")
}

fn main() {
    let n = install().expect("install failed")
    println(n)
}
