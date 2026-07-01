# Strings

_Immutable UTF-8 text; all operations are methods returning new strings; character-indexed._

A `string` is an immutable, reference-typed UTF-8 value. Every operation is a method
that returns a \*new\* string — strings are never mutated in place. Indexing helpers
(`len`, `slice`, `find`) work in **characters**, not bytes (`bytes_len` gives the
byte count). There is no string interpolation in v1 — use `fmt`.


## Building strings

```rust
let log = "hp: " + str(99)        // + concatenates; str() converts any value
let msg = fmt("{} of {}", 3, 10)  // "3 of 10" — {} placeholders, {{ }} escape
```

`+` concatenates two strings. `str(any) -> string` converts any value (using its `Display` impl when present). `fmt(template, args…)` substitutes into `{}` placeholders; the placeholder count is checked at compile time for literal templates.

## Methods

All string methods return new values. Searching, splitting/joining, reshaping and slicing are covered:

```rust
let parts = "a,b,c".split(",")          // List[string] ["a","b","c"]
let up    = "hello".to_upper()          // "HELLO"
let yes   = "hello".starts_with("he")   // true
let n     = "x7".slice(1, 2)            // "7" — character-indexed
let maybe = "42".parse_int()            // Option[int] -> Some(42)
```

See [string methods](../references/fact_string_methods.md) for the full list.

## Related

- [Values & Types](../references/concept_values.md)

- [Containers: List & Map](../references/concept_containers.md)

- [string methods](../references/fact_string_methods.md)

[← Back to SKILL.md](../SKILL.md)
