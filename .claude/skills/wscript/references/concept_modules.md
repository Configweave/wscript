# Modules & use

_use imports host modules, items, and other script files; registered types are ambient; the prelude is always available._

`use module` imports a host-registered module; `use module::item` imports one item.
Registered **types** are ambient — no `use` needed for type names; `use` only governs
functions and constants.


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

## Multi-file scripts

`use` also imports **other script files**:

```rust
use helpers                       // helpers.wscript, next to this file
                                  // (or under wscript.toml's src_roots)
use helpers::double               // import one fn into scope
use "./sub/geo.wscript" as geo    // explicit path, relative to this file
```

Rules: a registered host module wins over a file of the same name. Each file is a module named by its stem (or `as` alias) — names are unique program-wide. Cyclic imports are fine (there are no top-level statements, so no initialization order). Types declared in any file are ambient across the program, like host types; functions are file-scoped and reached via `module::fn`. Only the \*entry\* file's fns are callable by the host. The whole graph compiles into one unit — diagnostics and stack traces name the right file.

> [!NOTE]
> **The prelude**
> Always available, no import: `print println str fmt same weak int float`. See the Prelude fact.

## Related

- [Embedding in Rust](../references/concept_embedding.md)

- [Functions & Closures](../references/concept_functions.md)

- [Prelude (always available, no import)](../references/fact_prelude_functions.md)

[← Back to SKILL.md](../SKILL.md)
