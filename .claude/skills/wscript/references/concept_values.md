# Values & Types

_Five primitive value types; everything else is a reference type. No implicit conversion, no truthiness._

wscript is statically typed with local inference. The **primitives** are value
types, copied when assigned. Everything else — `string`, structs, enums, `List[T]`,
`Map[K, V]`, function values, `weak[T]` — is a **reference type**, where assignment
copies the \*reference\*, not the data.


## Primitives

| Type | Examples | Notes |
| --- | --- | --- |
| `int` | `42`, `-7`, `0xFF`, `1_000_000` | 64-bit signed, wrapping |
| `float` | `3.14`, `1e9` | 64-bit IEEE |
| `bool` | `true`, `false` | no truthiness — conditions must be `bool` |
| `char` | `'a'`, `'\n'`, `'\u{1F600}'` | a Unicode scalar |
| `unit` | `()` | the empty type; an omitted return is `unit` |

## Bindings & inference

`let` introduces a binding. Annotations are allowed anywhere on `let`, required nowhere — inference is local. `let mut` is not a thing: any binding is mutable through its reference.

```rust
let x = 5                  // inferred: int
let name: string = "wil"   // annotation allowed
let pi = 3.14              // inferred: float
```

## No implicit conversion, no truthiness

`1 + 2.0` is a **type error** — there is no implicit numeric widening. Convert explicitly with `int(x)` (truncates) and `float(x)`. Conditions must be `bool`; there is no truthiness.

```rust
let total = 1 + int(2.0)   // ok: int(2.0) == 2
let r = float(3) / 2.0     // ok
// if 1 { }                // ERROR — condition must be bool
```

> [!NOTE]
> **Conversions**
> `int(int|float|char)` (float truncates; char gives the code point) and `float(int|float)` are prelude functions, always available.

## Related

- [Strings](../references/concept_strings.md)

- [Reference Semantics](../references/concept_reference_semantics.md)

- [Primitive types](../references/fact_primitive_types.md)

- [Reference types](../references/fact_reference_types.md)

- [The Type System](../references/concept_type_system.md)

[← Back to SKILL.md](../SKILL.md)
