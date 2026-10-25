// `expect` on `None` faults with its message alone: `None` has no payload.
// error: no config found

fn find() -> Option[string] {
    None
}

fn main() {
    let c = find().expect("no config found")
    println(c)
}
