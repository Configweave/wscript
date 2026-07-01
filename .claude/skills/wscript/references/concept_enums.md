# Enums

_Tagged unions with unit, tuple and struct variants, consumed by match._

An `enum` is a tagged union. Variants come in three shapes: unit, tuple, and struct.

```rust
enum Event {
    Quit,                        // unit variant
    Key(char),                   // tuple variant
    Click { x: int, y: int },    // struct variant
}

let e = Event::Click { x: 10, y: 20 }
```

Construct a variant with `Enum::Variant`. Enums are consumed with [`match`](../references/concept_pattern_matching.md), which checks exhaustiveness at compile time. `Option[T]` and `Result[T, E]` are the two built-in enums.

## Related

- [Pattern Matching](../references/concept_pattern_matching.md)

- [Structs & Methods](../references/concept_structs.md)

- [Option, Result & ?](../references/concept_options_results.md)

[← Back to SKILL.md](../SKILL.md)
