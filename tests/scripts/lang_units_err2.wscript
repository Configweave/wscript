// A suffix shared by two families needs an expected type to resolve.
// error: `s` is a unit of `Duration` and `Length`

units Duration: int { ns = 1, s = 1_000_000_000 }
units Length: float { m = 1.0, s = 1_000.0 }

fn main() {
    let ambiguous = 5s
    println(ambiguous)
}
