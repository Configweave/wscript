// A bad format spec in a literal template is a compile error.
// error: invalid format spec

fn main() {
    println(fmt("{:q}", 1))
}
