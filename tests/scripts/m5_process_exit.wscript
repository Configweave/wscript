// process::exit terminates with the given code; nothing after it runs.
// expect: before
// exit: 3

use process

fn main() -> int {
    println("before")
    process::exit(3)
    println("after")
    0
}
