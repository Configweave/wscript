// Statement termination: continuation in parens, trailing operator,
// leading-dot chains, else on next line.
// expect: 21
// expect: HELLO
// expect: yes

fn add3(a: int, b: int, c: int) -> int { a + b + c }

fn main() {
    let x = add3(1,
        2,
        3) + add3(4,
        5, 6)
    println(x)
    let s = "hello"
        .to_upper()
    println(s)
    if x == 21 {
        println("yes")
    }
    else {
        println("no")
    }
}
