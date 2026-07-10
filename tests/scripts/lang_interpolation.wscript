// String interpolation: full expressions in holes, escapes, fmt-template
// compatibility, custom Display, nesting.
// expect: hp: 70 of 100
// expect: sum: 12, halved: 6
// expect: literal {braces} and {} and {:>8} stay literal
// expect: items: [1, 2, 3] (len 3)
// expect: point is (3, 4)
// expect: nested: outer-inner-5
// expect: 3 of 10

#[derive(Clone)]
struct Player { name: string, hp: int }

struct Point { x: int, y: int }

impl Display for Point {
    fn fmt(self) -> string { "({self.x}, {self.y})" }
}

fn main() -> int {
    let p = Player { name: "wil", hp: 70 }
    let max = 100
    println("hp: {p.hp} of {max}")

    // full expressions, method calls, arithmetic
    let xs = [3, 4, 5]
    println("sum: {xs.sum()}, halved: {xs.sum() / 2}")

    // {{ }} escapes; {} and {:spec} stay literal (fmt templates keep working)
    println("literal {{braces}} and {} and {:>8} stay literal")

    // structural display + len
    println("items: {[1, 2, 3]} (len {xs.len()})")

    // custom Display impls render in holes
    println("point is {Point { x: 3, y: 4 }}")

    // an interpolated string inside a hole
    let n = 5
    println("nested: {"outer-{"inner-{n}"}"}")

    // fmt still works for dynamic templates
    let template = "{} of {}"
    println(fmt(template, 3, 10))
    0
}
