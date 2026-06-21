// Structs, impl blocks, methods, reference semantics.
// expect: 7
// expect: Point { x: 3, y: 4 }
// expect: 70
// expect: true

struct Point { x: int, y: int }

impl Point {
    fn manhattan(self) -> int { abs(self.x) + abs(self.y) }
    fn shift(self, dx: int, dy: int) {
        self.x = self.x + dx
        self.y = self.y + dy
    }
}

fn abs(n: int) -> int { if n < 0 { -n } else { n } }

struct Player { name: string, hp: int }

fn main() {
    let p = Point { x: -1, y: 4 }
    p.shift(4, 0)
    println(p.manhattan())
    println(p)

    let pl = Player { name: "wil", hp: 100 }
    let alias = pl
    alias.hp = alias.hp - 30
    println(pl.hp)
    println(same(pl, alias))
}
