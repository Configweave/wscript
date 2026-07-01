# Containers: List & Map

_Built-in List\[T\] and Map\[K, V\] with literal syntax and rich method sets._

wscript ships two built-in, special-cased generic containers: `List[T]` and `Map[K, V]`. (User-defined generics do not exist in v1; these two are the exception.)

## Lists

```rust
let xs = [1, 2, 3]                 // List[int]
xs.push(4)
xs[0]                              // faults if out of bounds
xs.get(99)                         // Option[int] — never faults
xs.map(|x| x * 2).filter(|x| x > 2).fold(0, |a, x| a + x)
```

## Maps

A map literal is written `#{ ... }`. Keys are `int`, `bool`, `char` or `string`.

```rust
let ages = #{ "alice": 30, "bob": 25 }   // Map[string, int]
ages["carol"] = 22                 // insert or overwrite
ages["nope"]                       // faults — use .get for an Option
ages.get("nope")                   // Option[int] -> None
```

> [!NOTE]
> **Index vs get**
> `xs[i]` and `m[k]` fault on a missing element; `.get(...)` returns an `Option` instead. Prefer `.get` where absence is expected.

See [List methods](../references/fact_list_methods.md) and [Map methods](../references/fact_map_methods.md) for the full surface.

## Related

- [Strings](../references/concept_strings.md)

- [Control Flow](../references/concept_control_flow.md)

- \[List\[T\] methods\](fact_list_methods)

- \[Map\[K, V\] methods\](fact_map_methods)

[← Back to SKILL.md](../SKILL.md)
