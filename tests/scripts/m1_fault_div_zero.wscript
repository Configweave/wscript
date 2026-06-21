// Faults are trappable errors with a span, not panics.
// error: division by zero

fn main() -> int {
    let zero = 0
    10 / zero
}
