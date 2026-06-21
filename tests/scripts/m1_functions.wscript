// Functions, recursion, unit returns, semicolons optional but allowed.
// expect: 120
// expect: done

fn fact(n: int) -> int {
    if n <= 1 { 1 } else { n * fact(n - 1) }
}

fn say(msg: string) { println(msg); }

fn main() {
    println(fact(5));
    say("done")
}
