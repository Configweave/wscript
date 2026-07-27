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

There is no implicit numeric conversion and no truthiness (see [Values & Types](../references/concept_values.md)).

[Unit families](../references/concept_units.md) extend that discipline to numbers that mean something: `units Duration: int { ns = 1, ms = 1_000_000 }` gives a distinct type whose values are written `500ms` and stored in base units, so a timespan and a byte count cannot be added. Like generics, they are erased — a unit value is its backing number at runtime.

## Generic functions

Top-level functions can declare type parameters, with the built-in bounds `Eq`, `Ord` and `Clone` (`Ord` implies `Eq`):

```rust
fn identity[T](x: T) -> T { x }
fn max_of[T: Ord](a: T, b: T) -> T { if a > b { a } else { b } }
fn dup[T: Clone](x: T) -> T { x.clone() }
fn head[T](xs: List[T]) -> Option[T] { xs.first() }
```

Instantiation is inferred at the call site from argument types (and the expected type — `let xs: List[int] = empty_of()` works); there is no explicit `f[int](...)` syntax. Inside the body a `T` value supports moving, storing in containers, matching, `print`/`str`/interpolation, and whatever its bounds grant: `==` under `Eq`, comparisons (plus `sort`/`min`/`max`/`contains`) under `Ord`, `.clone()` under `Clone`. Everything else — arithmetic, field access, methods — needs a concrete type. Every type parameter must appear in the signature.

Generics are \*erased\*: one compiled function serves all instantiations, so there is no code bloat and no host-visible monomorphization. The flip side: hosts cannot call generic fns directly — wrap them in a monomorphic fn. Generic structs/enums, methods and user-trait bounds are planned follow-ups; the built-in `List[T]` / `Map[K,V]` containers are special-cased.

## Related

- [Functions & Closures](../references/concept_functions.md)

- [Values & Types](../references/concept_values.md)

- [Embedding in Rust](../references/concept_embedding.md)

- [Unit Families](../references/concept_units.md)

[← Back to SKILL.md](../SKILL.md)
