// Unknown associated function is a targeted compile error.
// error: has no associated function

struct P { x: int }

impl P {
    fn new(x: int) -> P { P { x: x } }
}

fn main() {
    let p = P::nope(1)
}
