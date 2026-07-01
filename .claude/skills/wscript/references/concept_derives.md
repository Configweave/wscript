# Derives

_#\[derive(...)\] generates Eq, Ord, Display and Clone impls._

`#[derive(...)]` auto-generates trait impls for a struct or enum.

```rust
#[derive(Eq, Ord, Display, Clone)]
struct Vec2 { x: int, y: int }
```

| Derive | Generates |
| --- | --- |
| `Eq` | structural `==` / `!=` |
| `Ord` | structural ordering; custom form is `fn cmp(self, other: Self) -> int` (-1/0/1) |
| `Display` | a debug-ish `str(x)` / `{}` rendering |
| `Clone` | a deep copy via `x.clone()` |

Derived `Eq`/`Ord` are structural; `Display` is debug-ish; `Clone` is a deep copy.

## Related

- [Operator Overloading](../references/concept_operators.md)

- [Traits](../references/concept_traits.md)

- [Reference Semantics](../references/concept_reference_semantics.md)

[← Back to SKILL.md](../SKILL.md)
