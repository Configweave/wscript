# Primitive types

The five value types, copied on assignment.

| Type | Example | Notes |
| --- | --- | --- |
| `int` | `42`, `-7`, `0xFF`, `1_000_000` | 64-bit signed, wrapping arithmetic |
| `float` | `3.14`, `1e9` | 64-bit IEEE 754 |
| `bool` | `true`, `false` | no truthiness — conditions must be `bool` |
| `char` | `'a'`, `'\n'`, `'\u{1F600}'` | a single Unicode scalar value |
| `unit` | `()` | the empty type; an omitted return type is `unit` |

There is no implicit numeric conversion: `1 + 2.0` is a type error. Convert with `int(x)` (float truncates; char gives the code point) and `float(x)`.

## Related

- [Values & Types](../references/concept_values.md)

- [Reference types](../references/fact_reference_types.md)

[← Back to SKILL.md](../SKILL.md)
