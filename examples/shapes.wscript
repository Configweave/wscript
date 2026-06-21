// M3 example: traits, dyn dispatch, operator traits, derives, weak refs.

trait Shape {
    fn area(self) -> float
    fn describe(self) -> string
}

#[derive(Eq, Display, Clone)]
struct Rect { w: float, h: float }

#[derive(Eq, Display, Clone)]
struct Circle { r: float }

impl Shape for Rect {
    fn area(self) -> float { self.w * self.h }
    fn describe(self) -> string { fmt("rect {}x{}", self.w, self.h) }
}

impl Shape for Circle {
    fn area(self) -> float { 3.14159265 * self.r * self.r }
    fn describe(self) -> string { fmt("circle r={}", self.r) }
}

impl Add for Rect {
    fn add(self, other: Rect) -> Rect {
        Rect { w: self.w + other.w, h: self.h + other.h }
    }
}

fn total_area(shapes: List[dyn Shape]) -> float {
    shapes.fold(0.0, |acc, s| acc + s.area())
}

fn main() -> int {
    let shapes: List[dyn Shape] = [
        Rect { w: 2.0, h: 3.0 },
        Circle { r: 1.0 },
        Rect { w: 1.0, h: 1.0 },
    ]
    for s in shapes {
        println(s.describe())
    }
    println(fmt("total: {}", total_area(shapes)))

    let a = Rect { w: 1.0, h: 2.0 }
    let b = a + Rect { w: 2.0, h: 1.0 }
    println(b)
    println(a == a.clone())
    0
}
