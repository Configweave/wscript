# Strings

_Immutable UTF-8 text; all operations are methods returning new strings; character-indexed._

A `string` is an immutable, reference-typed UTF-8 value. Every operation is a method
that returns a \*new\* string — strings are never mutated in place. Indexing helpers
(`len`, `slice`, `find`) work in **characters**, not bytes (`bytes_len` gives the
byte count).


## Building strings

```rust
let log = "hp: {99}"                     // interpolation: {expr} embeds any expression
let who = "player {p.name} at {x + 1}"   // rendered like str()
let cat = "hp: " + str(99)               // + concatenation and str() also work
let msg = fmt("{} of {}", 3, 10)         // fmt: for DYNAMIC templates
let hex = fmt("{:>8} {:.2} {:x}", "hi", 3.14159, 255)  // and format specs
```

**Interpolation:** every string literal may hold `{expr}` holes — one full expression each (fields, calls, arithmetic, even struct literals and nested interpolated strings), rendered like `str()`. Holes are real code — write inner strings with plain quotes (`{s.split(",")}`), not `\"` escapes. Escape literal braces as `{{` and `}}`. For fmt-template compatibility, `{}` and `{:spec}` stay \*literal\* text — so `fmt` templates are unchanged, but regex quantifiers and inline JSON need escaping: `"[0-9]{{4}}"`. Format specs inside holes (`{x:.2}`) are reserved — use `fmt`.

`+` concatenates two strings. `str(any) -> string` converts any value (using its `Display` impl when present). `fmt(template, args…)` substitutes into `{}` placeholders, with optional format specs (`{:>8}`, `{:.2}`, `{:x}` — see the [Prelude fact](../references/fact_prelude_functions.md)); placeholder count and specs are checked at compile time for literal templates.

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
