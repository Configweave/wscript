// Unit families: nominal types backed by int/float, stored in base units.
// expect: 500
// expect: 1500
// expect: 2000
// expect: 3000
// expect: 1500
// expect: 3
// expect: 500
// expect: true
// expect: false
// expect: -1500
// expect: 250
// expect: 250000
// expect: 4096
// expect: 4194304
// expect: 0
// expect: 90
// expect: 1500ms
// expect: frame 1500ms, budget 4MiB
// expect: 1500ms
// expect: 0ns
// expect: 1B
// expect: 3.141592653589793rad
// expect: 28.64788975654116deg
// expect: 90min
// expect: half a second
// expect: 2500
// expect: 2000
// expect: 666
// expect: 2s left
// expect: 512MiB
// expect: 1500000000
// expect: [500000000, 1500000000]

units Duration: int {
    ns  = 1
    us  = 1_000
    ms  = 1_000 * us
    s   = 1_000 * ms
    min = 60 * s
    h   = 60 * min
}

// Comma-separated entries work too.
units ByteSize: int { B = 1, KiB = 1_024, MiB = 1_024 * KiB, GiB = 1_024 * MiB }

units Angle: float {
    rad = 1.0
    deg = 0.017453292519943295
}

struct Budget {
    window: Duration,
    cap: ByteSize,
}

fn double(d: Duration) -> Duration { d * 2 }

fn main() {
    let timeout: Duration = 500ms
    let frame = 1.5s
    println(timeout.ms)
    println(frame.ms)

    // Arithmetic stays inside the family.
    println((timeout + frame).ms)
    println((frame * 2).ms)
    println((3 * timeout).ms)
    println(frame / timeout)
    println((frame / 3).ms)
    println(frame > timeout)
    println(frame == timeout)
    println((-frame).ms)

    // Construction from a value that isn't a literal.
    let n = 250
    let t = Duration::ms(n)
    println(t.ms)
    println(t.us)

    let budget = 4MiB
    println(budget.KiB)
    println(budget.B)

    // Extraction truncates on int-backed families.
    println(1s.min)
    println(1.5h.min)

    // Display renders in the largest unit that divides exactly.
    println(frame)
    println("frame {frame}, budget {budget}")
    println(str(frame))
    println(0s)
    println(1B)
    println(180deg)
    println(0.5rad)
    println(1.5h)

    match 500ms {
        500ms => println("half a second"),
        _ => println("other"),
    }

    // Compound assignment goes through the same rules.
    let mut_a = 1s
    mut_a += 1.5s
    println(mut_a.ms)
    mut_a -= 500ms
    println(mut_a.ms)
    mut_a /= 3
    println(mut_a.ms)

    // Unit-typed struct fields and fn signatures.
    let b = Budget { window: double(1s), cap: 512MiB }
    println("{b.window} left")
    println(b.cap)

    // Base-unit extraction gives the raw stored number.
    println(frame.ns)

    // Erased inside containers: a documented limitation.
    println([timeout, frame])
}
