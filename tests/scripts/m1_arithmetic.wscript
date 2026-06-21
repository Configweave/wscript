// Arithmetic, precedence, floats, comparison, logic.
// expect: 14
// expect: 2
// expect: 3.5
// expect: true
// expect: false
// expect: true

fn main() {
    println(2 + 3 * 4)
    println(7 % 5)
    println(7.0 / 2.0)
    println(1 < 2 && 2 <= 2)
    println(!(1 == 1))
    println("abc" < "abd" || false)
}
