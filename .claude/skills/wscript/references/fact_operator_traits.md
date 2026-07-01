# Operator overloading traits

Operators dispatch through these built-in traits. Implement the trait to overload the operator.

| Trait | Operator / use |
| --- | --- |
| `Add` `Sub` `Mul` `Div` `Rem` | `+` `-` `*` `/` `%` |
| `Neg` | unary `-` |
| `Eq` | `==` / `!=` (REQUIRED for `==` on structs/enums) |
| `Ord` | `<` `<=` `>` `>=`; custom form `fn cmp(self, other: Self) -> int` |
| `Display` | `str(x)` and `{}` formatting |
| `Index` | `x[i]` — read-only in v1 |

There is no implicit identity equality — use `same(a, b)` for reference identity.

## Related

- [Operator Overloading](../references/concept_operators.md)

- [Derives](../references/concept_derives.md)

[← Back to SKILL.md](../SKILL.md)
