# The Type System

_Static typing with local inference; annotations only on function signatures; host APIs checked at compile time._

wscript is statically typed. Inference is **local**: annotations are required on
function signatures and nowhere else. Type errors — including misuse of
host-registered APIs — are caught when the script \*compiles\*, not when it runs. This
is the language's defining property versus untyped embeddable languages.


```rust
// Given a host registration of `term::print_at(x: int, y: int, s: string)`:
term::print_at("oops", 1, 2)   // COMPILE error with a span and help message
```

There is no implicit numeric conversion and no truthiness (see [Values & Types](../references/concept_values.md)). Generics are not user-definable in v1 — only the built-in `List[T]` / `Map[K,V]` are generic.

## Related

- [Functions & Closures](../references/concept_functions.md)

- [Values & Types](../references/concept_values.md)

- [Embedding in Rust](../references/concept_embedding.md)

[← Back to SKILL.md](../SKILL.md)
