# Operator Overloading

_Operators go through built-in traits; == on structs/enums requires an Eq impl._

Operator overloading goes through built-in traits: `Add Sub Mul Div Rem Neg Eq Ord Display Index`. See [operator traits](../references/fact_operator_traits.md) for the full mapping.

```rust
impl Add for Vec2 {
    fn add(self, other: Vec2) -> Vec2 {
        Vec2 { x: self.x + other.x, y: self.y + other.y }
    }
}
```

> [!NOTE]
> **No implicit identity equality**
> `==` on structs/enums REQUIRES an `Eq` impl — there is no implicit identity equality. Use `same(a, b)` for reference identity instead. `Index` is read-only in v1.

## Related

- [Traits](../references/concept_traits.md)

- [Derives](../references/concept_derives.md)

- [Operator overloading traits](../references/fact_operator_traits.md)

[← Back to SKILL.md](../SKILL.md)
