# Reference types

Everything that is not a primitive is a reference type: assignment copies the reference, not the data.

| Type | Notes |
| --- | --- |
| `string` | immutable UTF-8; methods return new strings; character-indexed |
| structs | named field records, mutated in place |
| enums | tagged unions; `Option[T]` / `Result[T,E]` are built in |
| `List[T]` | growable ordered sequence (built-in generic) |
| `Map[K, V]` | hash map; keys are `int`/`bool`/`char`/`string` (built-in generic) |
| function values | closures capture by reference |
| `weak[T]` | a weak reference; `.upgrade() -> Option[T]` |

## Related

- [Reference Semantics](../references/concept_reference_semantics.md)

- [Primitive types](../references/fact_primitive_types.md)

[← Back to SKILL.md](../SKILL.md)
