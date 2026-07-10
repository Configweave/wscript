// A concrete type that doesn't satisfy a generic fn's bound is E0253.
// error: does not satisfy the bound `T: Ord`

struct NoOrd { n: int }

fn max_of[T: Ord](a: T, b: T) -> T {
    if a > b { a } else { b }
}

fn main() {
    let m = max_of(NoOrd { n: 1 }, NoOrd { n: 2 })
}
