// Derives: Eq, Ord, Display, Clone (deep).
// expect: true
// expect: false
// expect: true
// expect: P { x: 1, y: [2, 3] }
// expect: [2, 3, 4]
// expect: [2, 3]
// expect: false

#[derive(Eq, Ord, Display, Clone)]
struct P { x: int, y: List[int] }

fn main() {
    let a = P { x: 1, y: [2, 3] }
    let b = P { x: 1, y: [2, 3] }
    println(a == b)            // structural equality
    println(same(a, b))        // distinct objects
    println(a < P { x: 2, y: [] })
    println(a)

    // plain assignment never clones (PRD §3.8); .clone() is deep
    let copy = a.clone()
    a.y.push(4)
    println(a.y)
    println(copy.y)
    println(a == b)
}
