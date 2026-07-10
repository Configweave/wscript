// time module: clocks, elapsed, sleep, ISO formatting. Only deterministic
// assertions — no wall-clock-sensitive expectations.
// expect: true
// expect: true
// expect: true
// expect: 1970-01-01T00:00:00Z
// expect: 2000-02-29T12:34:56Z

use time

fn main() -> int {
    println(time::now_unix() > 1.0e9)
    println(time::now_millis() > 1000000000000)

    let t = time::instant()
    time::sleep(5)
    println(time::elapsed(t) >= 0.0)

    println(time::format_iso(0.0))
    println(time::format_iso(951827696.0))
    0
}
