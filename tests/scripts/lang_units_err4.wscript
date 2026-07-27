// An int-backed family only takes values that land on a whole base unit.
// error: is not a whole number of `ns`

units Duration: int { ns = 1, us = 1_000, ms = 1_000 * us }

fn main() {
    let sub_ns = 0.5ns
    println(sub_ns)
}
