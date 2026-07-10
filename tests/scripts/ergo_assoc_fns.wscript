// Associated functions: no-self fns in inherent impl blocks, called as
// Type::func(...). Enum variants win over assoc fns on name collision.
// expect: (0, 0)
// expect: (3, 4)
// expect: 7
// expect: made-by-variant
// expect: made-by-assoc

#[derive(Display)]
struct Point { x: int, y: int }

impl Display for Point {
    fn fmt(self) -> string { fmt("({}, {})", self.x, self.y) }
}

impl Point {
    fn origin() -> Point { Point { x: 0, y: 0 } }
    fn new(x: int, y: int) -> Point { Point { x: x, y: y } }
    fn manhattan(self) -> int { self.x + self.y }
}

enum Marker {
    Make(string),      // variant named like a factory
}

impl Marker {
    fn make() -> Marker { Marker::Make("made-by-assoc") }
}

fn main() -> int {
    println(Point::origin())
    let p = Point::new(3, 4)
    println(p)
    println(p.manhattan())         // methods still work alongside

    // Variant wins: `Marker::Make` is the tuple variant...
    let v = Marker::Make("made-by-variant")
    // ...and the lowercase assoc fn is reachable too.
    let a = Marker::make()
    match v { Marker::Make(s) => println(s) }
    match a { Marker::Make(s) => println(s) }
    0
}
