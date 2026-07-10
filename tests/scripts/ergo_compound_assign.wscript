// Compound assignment: locals, captured vars, fields, list/map elements,
// string concat, user Add impls — and the place evaluates exactly once.
// expect: 15
// expect: 2.5
// expect: ab!
// expect: 3
// expect: [10, 22]
// expect: 8
// expect: (4, 6)
// expect: index computed
// expect: [0, 100]

struct Counter { n: int }

#[derive(Display)]
struct Vec2 { x: int, y: int }

impl Add for Vec2 {
    fn add(self, other: Vec2) -> Vec2 {
        Vec2 { x: self.x + other.x, y: self.y + other.y }
    }
}

impl Display for Vec2 {
    fn fmt(self) -> string { fmt("({}, {})", self.x, self.y) }
}

fn idx(marker: List[int]) -> int {
    println("index computed")
    marker[0] + 1
}

fn main() -> int {
    // locals: int, float, string
    let n = 10
    n += 2
    n *= 5
    n /= 4
    println(n)                    // ((10+2)*5)/4 = 15

    let f = 1.0
    f += 1.5
    println(f)

    let s = "a"
    s += "b"
    s += "!"
    println(s)

    // captured variable inside a closure
    let c = 0
    let bump = || { c += 3 }
    bump()
    println(c)

    // list element and field
    let xs = [10, 20]
    xs[1] += 2
    println(xs)

    let counter = Counter { n: 5 }
    counter.n += 3
    println(counter.n)

    // user Add impl
    let v = Vec2 { x: 1, y: 2 }
    v += Vec2 { x: 3, y: 4 }
    println(v)

    // evaluate-once: the index expression runs a fn that prints — one line
    let ys = [0, 0]
    ys[idx([0])] += 100
    println(ys)
    0
}
