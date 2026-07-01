# Interop: Data vs Opaque Types

_#\[derive(Script)\] exposes types; data types cross by value, opaque types cross by handle._

`#[derive(Script)]` marks a Rust type for script exposure. The kind of crossing depends on whether it is a data or an opaque type.

## Data types — cross by value

```rust
#[derive(Script)]                 // DATA type: fields visible in script,
struct Point { x: i64, y: i64 }   // constructible and matchable

#[derive(Script)]
enum Event {                      // enums derive too, incl. payloads
    Quit,
    Key(char),
    Click { x: i64, y: i64 },
}
```

The script constructs a `Point`, your function receives a real `Point`. Mutations inside the script to its copy do not write back into yours.

## Opaque types — cross by handle

```rust
#[derive(Script)]
#[script(opaque)]                 // HANDLE type: no fields, methods only
struct Pane { title: String }

m.ty::<Pane>()
    .method("title", |p: &Pane| p.title.clone())
    .method("split", |p: &mut Pane, vertical: bool| { /* ... */ });
```

The script holds a reference to the live Rust value; `&mut` methods mutate it in place. Scripts cannot touch fields or construct opaque values.

> [!NOTE]
> **Ambient types & registration**
> Registered types are ambient in the script's type namespace — `use` only governs functions and constants. A type appearing only in a `ScriptFn` boundary still needs `Context::new().register_type::<KeyEvent>()`. `Shared<T>` is a host-side handle to a live script value; aliasing violations surface as `Err`, never panics.

## Related

- [Embedding in Rust](../references/concept_embedding.md)

- [Interface Files & wscript.toml](../references/concept_interface_files.md)

- [Reference Semantics](../references/concept_reference_semantics.md)

[← Back to SKILL.md](../SKILL.md)
