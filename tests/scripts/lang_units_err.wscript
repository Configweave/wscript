// Unit families do not mix: adding across families is a type error.
// error: expected `Duration`, found `ByteSize`

units Duration: int { ns = 1, ms = 1_000_000, s = 1_000 * ms }
units ByteSize: int { B = 1, KiB = 1_024 }

fn main() {
    let bad = 5s + 3KiB
    println(bad)
}
