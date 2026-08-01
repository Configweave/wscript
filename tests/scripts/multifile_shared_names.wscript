// Top-level fn names are file-scoped: two files may define `shared`, and
// each file's bare call binds to its own (#23).
// expect: entry: 111
// expect: other: 222

use "./modules/other.wscript" as other

fn shared() -> int { 111 }

fn main() {
    println("entry: " + str(shared()))
    println("other: " + str(other::from_other()))
}
