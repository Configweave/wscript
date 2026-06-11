// math module.
// expect: 3.0
// expect: 2.0
// expect: 8.0
// expect: 7
// expect: 1.0
// expect: true
// expect: true

use math

fn main() {
    println(math::abs(-3.0))
    println(math::sqrt(4.0))
    println(math::pow(2.0, 3.0))
    println(math::iclamp(99, 0, 7))
    println(math::lerp(0.0, 2.0, 0.5))
    println(math::PI > 3.14 && math::PI < 3.15)
    let r = math::rand()
    let n = math::rand_range(0, 10)
    println(r >= 0.0 && r < 1.0 && n >= 0 && n < 10)
}
