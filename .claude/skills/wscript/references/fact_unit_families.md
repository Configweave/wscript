# Unit family syntax and diagnostics

The surface of `units` declarations, in one place. See [Unit Families](../references/concept_units.md) for the walkthrough.

| Form | Meaning |
| --- | --- |
| `units D: int { ns = 1, ms = 1_000_000 }` | declare a family backed by `int`; `ns` is the base unit |
| `units A: float { rad = 1.0, deg = 0.0174 }` | float-backed; factors may be fractional |
| `500ms`, `1.5s`, `4MiB` | a literal with a unit suffix |
| `d.ms` | convert out of the family, to the backing type |
| `D::ms(n)` | convert into the family from a non-literal |
| `impl Display for D { ... }` | override the default rendering |
| `impl D { fn f(self) -> ... }` | inherent methods (may not shadow a unit name) |

`units` is a contextual keyword: it only introduces a declaration at item position followed by a type name, so scripts may still use `units` as an ordinary identifier.

| Code | Reported when |
| --- | --- |
| `E0260` | a unit suffix belongs to two families and nothing fixes the expected type |
| `E0262` | unknown unit suffix |
| `E0264` | the backing type is not `int` or `float` |
| `E0265` | a unit is declared twice in one family |
| `E0266` | a factor is zero, negative, non-finite, or overflows |
| `E0267` | the family has no base unit, or more than one |
| `E0268` | a factor is not a constant expression over literals and earlier units |
| `E0269` | a literal does not land on a whole base unit, or overflows the family |
| `E0234` | an operator would derive a new dimension (`5s * 2ms`, `5s / 3KiB`) |
| `E0220` | operands belong to different families |
| `E0206` | an operator trait impl, or a method shadowing a unit name |

## Related

- [Unit Families](../references/concept_units.md)

- [Primitive types](../references/fact_primitive_types.md)

- [The Type System](../references/concept_type_system.md)

[← Back to SKILL.md](../SKILL.md)
