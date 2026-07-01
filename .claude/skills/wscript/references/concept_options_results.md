# Option, Result & ?

_Option\[T\] and Result\[T, E\] are always available; ? early-returns and composes across the host boundary._

`Option[T]` and `Result[T, E]` are always available — no import. The `?` operator early-returns the `None`/`Err`, exactly as in Rust, and composes across the host boundary (host errors arrive as `Err`).

```rust
fn parse_double(s: string) -> Result[int, string] {
    let Some(n) = s.parse_int() else { return Err("bad number") }
    Ok(n * 2)
}

fn sum(a: string, b: string) -> Result[int, string] {
    Ok(parse_double(a)? + parse_double(b)?)
}
```

## Methods

`Option`: `is_some is_none unwrap unwrap_or expect`. `Result`: `is_ok is_err unwrap unwrap_or unwrap_err expect`. `unwrap()` on the empty case is a trappable [fault](../references/concept_faults.md), not a host panic. See [Option/Result methods](../references/fact_option_result_methods.md).

> [!NOTE]
> **Prefer Option-returning APIs**
> Where failure is expected (a missing list index or map key), reach for the Option-returning form (`xs.get(i)`, `m.get(k)`) rather than the faulting index.

## Related

- [Enums](../references/concept_enums.md)

- [Pattern Matching](../references/concept_pattern_matching.md)

- [Option & Result methods](../references/fact_option_result_methods.md)

- [Faults](../references/concept_faults.md)

[← Back to SKILL.md](../SKILL.md)
