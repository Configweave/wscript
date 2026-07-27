# Unit Families

_`units Name: int { ... }` declares a numeric type with named units; values are stored in the base unit and erased at runtime._

A unit family is a nominal type backed by `int` or `float` with a named set of units and one base unit. Values are normalised to the base unit at compile time and stored as the plain backing number — a unit family is a compile-time newtype with no runtime cost. It stops a byte count being added to a timespan without asking scripts to carry the convention in their heads.

```rust
units Duration: int {
    ns  = 1                 // factor 1 -> this is the base unit
    us  = 1_000
    ms  = 1_000 * us        // factors may use units declared above them
    s   = 1_000 * ms
    min = 60 * s
    h   = 60 * min
}

units ByteSize: int { B = 1, KiB = 1_024, MiB = 1_024 * KiB, GiB = 1_024 * MiB }

units Angle: float {
    rad = 1.0
    deg = 0.017453292519943295
}
```

Entries are separated by newlines or commas. Exactly one unit must have the factor 1 — that is the base unit, and every value of the family is stored as a count of it. Factors are const-evaluated in declaration order from numeric literals, earlier units of the same family, and `+ - * /`; they must be positive and finite.

Write a value by suffixing a numeric literal, and read one back with the unit as a field. Both directions compile to a single constant multiply or divide.

```rust
let timeout: Duration = 500ms      // stored as 500_000_000 (ns)
let frame = 1.5s                   // suffix resolves on its own when unique
let budget = 4MiB

let ms: int = frame.ms             // 1500 — extraction, truncating
let raw: int = frame.ns            // 1500000000 — the stored number

let n = read_timeout()             // an int
let t = Duration::ms(n)            // build from a value that isn't a literal
```

A suffix resolves to a family by the expected type first, then by being unique among the families in scope. When two families share a unit name and there is no expected type, that is an error (E0260) — annotate the binding or use the `Family::unit(n)` form. A fractional literal in an int-backed family is accepted only if it lands on a whole base unit, so `0.5us` is fine and `0.5ns` is an error.

Arithmetic stays inside one family. A family combines with itself and scales by its backing number; dividing two values of the same family gives that plain number back.

| Expression | Result | Notes |
| --- | --- | --- |
| `d + d`, `d - d`, `d % d` | the family | both operands must be the same family |
| `-d` | the family | negation is the backing number's |
| `d * n`, `n * d`, `d / n` | the family | `n` is the backing `int`/`float` |
| `d / d` | backing type | a plain ratio |
| `d == d`, `d < d`, … | `bool` | comparison is the backing number's |
| `d += d`, `d *= n`, … | assignment | same rules; must land back in the family |

Everything else is a type error: `5s + 3MiB` mixes families, and `5m / 2s` would derive a new dimension. This release does not model dimensional analysis — see [what's not in v1](../references/fact_not_in_v1.md).

By default a value renders in the largest declared unit that names it cleanly: for int-backed families, the largest unit that divides the stored number exactly (so the rendering always round-trips); for float-backed families, the largest unit the value reaches. `impl Display for Duration` overrides it.

```rust
println(1500ms)            // 1500ms   (`s` does not divide it exactly)
println(2s)                // 2s
println(4MiB)              // 4MiB
println(0s)                // 0ns      (zero renders in the base unit)
println("took {frame}")    // took 1500ms
```

> [!WARNING]
> **Units are erased at runtime**
> A unit value IS its backing number in memory. The compiler renders it by family only where the static type is visible at the call site — a unit value inside a `List`, `Map` or `dyn`, or passed to a host function, prints as the raw base-unit number. At the embedding boundary a unit type appears as its backing `i64`/`f64`, so `fn tick(dt: Duration)` is called from Rust as `ScriptFn<(i64,), i64>` in base units.

> [!NOTE]
> **Operators cannot be overridden**
> A unit family already gets arithmetic, comparison, equality, ordering and cloning from its backing number, so `impl Add for Duration` is rejected — it could never be called. `Display` is the exception, and inherent `impl` blocks may add methods as long as they do not shadow a unit name. `#[derive(...)]` on a `units` declaration is rejected for the same reason.

## Examples

### Rate limiter — unit families for timespans and byte sizes

Two unit families keep timespans and byte counts apart at compile time. Values are written with suffixes, stored in base units (`ns`, `B`), and print in the largest unit that names them cleanly.

```rust
units Duration: int {
    ns  = 1
    us  = 1_000
    ms  = 1_000 * us
    s   = 1_000 * ms
    min = 60 * s
}

units ByteSize: int { B = 1, KiB = 1_024, MiB = 1_024 * KiB, GiB = 1_024 * MiB }

struct Limit {
    window: Duration,
    quota: ByteSize,
}

impl Limit {
    fn per_second(self) -> int {
        // `window / 1s` is a plain ratio, so this stays honest arithmetic.
        let secs = self.window / 1s
        if secs == 0 { self.quota.B } else { self.quota.B / secs }
    }
}

fn backoff(attempt: int, base: Duration) -> Duration {
    let wait = base
    let i = 0
    while i < attempt {
        wait *= 2
        i += 1
    }
    if wait > 30s { 30s } else { wait }
}

fn main() -> int {
    let limit = Limit { window: 1min, quota: 512MiB }
    println("window {limit.window}, quota {limit.quota}")
    println("{limit.per_second()} B/s")

    let attempt = 0
    while attempt < 4 {
        println("attempt {attempt}: wait {backoff(attempt, 250ms)}")
        attempt += 1
    }

    // Mixing families does not compile:
    //   limit.window + limit.quota   // error: expected Duration, found ByteSize
    0
}
```

**Expected:** Prints `window 1min, quota 512MiB`, then `8947848 B/s`, then a doubling backoff: 250ms, 500ms, 1s, 2s.

## Related

- [The Type System](../references/concept_type_system.md)

- [Primitive types](../references/fact_primitive_types.md)

- [Unit family syntax and diagnostics](../references/fact_unit_families.md)

- [Operator Overloading](../references/concept_operators.md)

- [Values & Types](../references/concept_values.md)

[← Back to SKILL.md](../SKILL.md)
