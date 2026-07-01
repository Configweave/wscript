# Modules & use

_use imports host-registered modules and items; registered types are ambient; the prelude is always available._

`use module` imports a host-registered module; `use module::item` imports one item.
Registered **types** are ambient — no `use` needed for type names; `use` only governs
functions and constants. Scripts are single files in v1 (script-to-script imports are
planned for v2).


```rust
use math
use fs
use process::run            // import a single item

fn main() -> int {
    let h = math::imax(0, -5)        // module::fn
    fs::write("out.txt", "hi")?
    0
}
```

> [!NOTE]
> **The prelude**
> Always available, no import: `print println str fmt same weak int float`. See the Prelude fact.

## Related

- [Embedding in Rust](../references/concept_embedding.md)

- [Functions & Closures](../references/concept_functions.md)

- [Prelude (always available, no import)](../references/fact_prelude_functions.md)

[← Back to SKILL.md](../SKILL.md)
