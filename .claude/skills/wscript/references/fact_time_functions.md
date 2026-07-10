# time module

Clocks and sleeping (capability: clock access, thread blocking).

| Function | Signature |
| --- | --- |
| `now_unix` | `() -> float` — wall-clock seconds since the unix epoch (negative before 1970) |
| `now_millis` | `() -> int` — wall-clock milliseconds since the unix epoch |
| `instant` | `() -> float` — monotonic clock: seconds since a process-wide anchor; instants are plain floats you subtract |
| `elapsed` | `(start: float) -> float` — seconds elapsed since an `instant()` reading |
| `sleep` | `(ms: int)` — blocks the VM thread; negative durations clamp to zero (not interruptible by fuel) |
| `format_iso` | `(ts: float) -> string` — UTC ISO-8601 from a unix timestamp in seconds, e.g. `"1970-01-01T00:00:00Z"`; sub-second precision truncated |

```rust
use time
let t0 = time::instant()
time::sleep(50)
println(fmt("took {:.3}s", time::elapsed(t0)))
```

## Related

- [Modules & use](../references/concept_modules.md)

[← Back to SKILL.md](../SKILL.md)
