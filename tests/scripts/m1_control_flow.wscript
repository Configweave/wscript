// if/else as expression, while, loop/break/continue, return.
// expect: 5
// expect: 10
// expect: 12
// exit: 3

fn classify(n: int) -> int {
    if n < 0 { return 0 }
    if n == 0 { 1 } else { 2 }
}

fn main() -> int {
    let x = if true { 5 } else { 6 }
    println(x)

    let i = 0
    let total = 0
    while i < 5 {
        i = i + 1
        if i % 2 == 0 { continue }
        total = total + i      // 1 + 3 + 5
    }
    let j = 0
    loop {
        j = j + 1
        if j >= 10 { break }
    }
    println(j)
    println(total + classify(-1) + classify(0) + classify(5))  // 9+0+1+2
    3
}
