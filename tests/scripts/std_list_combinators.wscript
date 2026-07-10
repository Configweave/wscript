// List combinators: any/all/find/position/count, sum, min/max, sort_by,
// map_indexed, zip_with; sort on Ord-implementing structs.
// expect: true
// expect: false
// expect: short-circuited after 2
// expect: true
// expect: false
// expect: 5
// expect: none
// expect: 2
// expect: 3
// expect: 10
// expect: 6.5
// expect: 0
// expect: 0.0
// expect: 1
// expect: 9
// expect: none
// expect: [9, 5, 3, 1]
// expect: ["0:a", "1:b"]
// expect: [11, 22]
// expect: 12

#[derive(Eq)]
struct Box { n: int }

impl Ord for Box {
    fn cmp(self, other: Box) -> int { self.n - other.n }
}

fn main() -> int {
    let xs = [1, 3, 5, 9]

    // any short-circuits: the predicate prints per call
    let calls = [0]
    println(xs.any(|x| { calls[0] = calls[0] + 1; x == 3 }))
    println(xs.any(|x| x == 4))
    println(fmt("short-circuited after {}", calls[0]))

    println(xs.all(|x| x % 2 == 1))
    println(xs.all(|x| x < 9))

    match xs.find(|x| x > 3) { Some(v) => println(v), None => println("none") }
    match xs.find(|x| x > 100) { Some(v) => println(v), None => println("none") }
    match xs.position(|x| x == 5) { Some(i) => println(i), None => println("none") }

    println(xs.count(|x| x >= 3))

    println([1, 2, 3, 4].sum())
    println([1.5, 2.0, 3.0].sum())
    let empty_i: List[int] = []
    let empty_f: List[float] = []
    println(empty_i.sum())
    println(empty_f.sum())

    match xs.min() { Some(v) => println(v), None => println("none") }
    match xs.max() { Some(v) => println(v), None => println("none") }
    match empty_i.min() { Some(v) => println(v), None => println("none") }

    let ys = [3, 9, 1, 5]
    ys.sort_by(|a, b| b - a)          // descending
    println(ys)

    println(["a", "b"].map_indexed(|i, s| fmt("{}:{}", i, s)))
    println([1, 2].zip_with([10, 20, 30], |a, b| a + b))

    // sort() now works on structs with an Ord impl
    let boxes = [Box { n: 2 }, Box { n: 1 }]
    boxes.sort()
    println(boxes.map(|b| str(b.n)).join(""))
    0
}
