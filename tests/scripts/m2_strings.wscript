// String methods.
// expect: 5
// expect: HELLO
// expect: ["ab", "cd"]
// expect: true
// expect: ell
// expect: Some(1)
// expect: x---
// expect: 3

fn main() {
    println("hello".len())
    println("hello".to_upper())
    println("ab,cd".split(","))
    println("hello".starts_with("he"))
    println("hello".slice(1, 4))
    println("hello".find("el"))
    println("x".pad_right(4, "-"))
    println("  3  ".trim().len() + 2)
}
