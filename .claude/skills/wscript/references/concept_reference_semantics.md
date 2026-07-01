# Reference Semantics

_Reference types alias rather than copy; mutation is visible through every alias; cloning is explicit._

This is the Lua/Python mental model, stated explicitly: assignment, argument passing,
and returns of **reference types** copy the \*reference\*, never the data. Two bindings
to the same object see each other's mutations. There is no borrow checker and no `&`
in the language at all — `self` in methods is always by reference.


## Aliasing

```rust
let p = Player { name: "wil", hp: 100 }
let alias = p              // same object, not a copy
alias.hp = 70
p.hp                       // 70 — mutation visible through both
same(p, alias)             // true — reference identity builtin
```

`same(a, b)` tests reference identity (the prelude builtin). It is distinct from `==`, which compares values and requires an `Eq` impl on structs/enums.

## Cloning is explicit

Plain assignment never clones. Deep copies are opt-in via a derived `Clone`:

```rust
#[derive(Clone)]
struct Config { values: List[int] }

let copy = config.clone()  // deep copy — independent of the original
```

> [!NOTE]
> **Primitives are values**
> int, float, bool, char and unit are copied on assignment — only reference types alias. See Values & Types.

## Related

- [Values & Types](../references/concept_values.md)

- \[Memory: Reference Counting & weak\[T\]\](concept_memory)

- [Structs & Methods](../references/concept_structs.md)

- [Prelude (always available, no import)](../references/fact_prelude_functions.md)

[← Back to SKILL.md](../SKILL.md)
