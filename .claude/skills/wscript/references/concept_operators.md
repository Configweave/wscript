# Operator Overloading

_Operators go through built-in traits; == on structs/enums requires an Eq impl._

Operator overloading goes through built-in traits: `Add Sub Mul Div Rem Neg Eq Ord Display Index`. See [operator traits](../references/fact_operator_traits.md) for the full mapping. Compound assignment `+= -= *= /= %=` works on any assignable place — variables, fields, list/map elements — evaluates the place once, and dispatches through the same traits, so user types get it via their `Add`/`Sub`/`Mul`/`Div`/`Rem` impls.

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

[Unit families](../references/concept_units.md) are the exception: they take their operators from the number they are stored in, so an operator impl for one is rejected.

## Related

- [Traits](../references/concept_traits.md)

- [Derives](../references/concept_derives.md)

- [Operator overloading traits](../references/fact_operator_traits.md)

- [Unit Families](../references/concept_units.md)

[← Back to SKILL.md](../SKILL.md)
