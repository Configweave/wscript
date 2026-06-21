// Operator traits: Add, Neg, Eq, Ord, Display, Index.
// expect: Vec2(4, 6)
// expect: Vec2(-1, -2)
// expect: true
// expect: false
// expect: true
// expect: 20

struct Vec2 { x: int, y: int }

impl Add for Vec2 {
    fn add(self, other: Vec2) -> Vec2 {
        Vec2 { x: self.x + other.x, y: self.y + other.y }
    }
}

impl Neg for Vec2 {
    fn neg(self) -> Vec2 { Vec2 { x: -self.x, y: -self.y } }
}

impl Eq for Vec2 {
    fn eq(self, other: Vec2) -> bool { self.x == other.x && self.y == other.y }
}

impl Ord for Vec2 {
    fn cmp(self, other: Vec2) -> int {
        let a = self.x * self.x + self.y * self.y
        let b = other.x * other.x + other.y * other.y
        if a < b { -1 } else if a > b { 1 } else { 0 }
    }
}

impl Display for Vec2 {
    fn fmt(self) -> string { fmt("Vec2({}, {})", self.x, self.y) }
}

struct Grid { cells: List[int] }

impl Index for Grid {
    fn index(self, i: int) -> int { self.cells[i] }
}

fn main() {
    let a = Vec2 { x: 1, y: 2 }
    let b = Vec2 { x: 3, y: 4 }
    println(a + b)
    println(-a)
    println(a == Vec2 { x: 1, y: 2 })
    println(a == b)
    println(a < b)
    let g = Grid { cells: [10, 20, 30] }
    println(g[1])
}
