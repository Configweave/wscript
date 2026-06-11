// Closures: capture by reference, mutation visibility, nesting, fn values.
// expect: 3
// expect: 10
// expect: 12
// expect: 7

fn make_counter() -> fn() -> int {
    let count = 0
    || {
        count = count + 1
        count
    }
}

fn apply(f: fn(int) -> int, x: int) -> int { f(x) }

fn add(a: int, b: int) -> int { a + b }

fn main() {
    let c = make_counter()
    c()
    c()
    println(c())

    let base = 4
    let scaled = apply(|x| x + base, 6)
    println(scaled)
    base = 6
    println(apply(|x| x + base, 6))

    let f = add
    println(f(3, 4))
}
