// M1 gate: recursive + iterative fibonacci.

fn fib(n: int) -> int {
    if n < 2 { n } else { fib(n - 1) + fib(n - 2) }
}

fn fib_iter(n: int) -> int {
    let a = 0
    let b = 1
    let i = 0
    while i < n {
        let next = a + b
        a = b
        b = next
        i = i + 1
    }
    a
}

fn main() -> int {
    println(fib(20))        // 6765
    println(fib_iter(20))   // 6765
    if fib(20) == fib_iter(20) { 0 } else { 1 }
}
