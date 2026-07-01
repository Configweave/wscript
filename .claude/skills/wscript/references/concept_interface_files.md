# Interface Files & wscript.toml

_.wscripti files declare the host API so the LSP and `wscript check` understand it._

The editor has never seen your host API. `ctx.write_interface("api.wscripti")` dumps
every registered signature into a `.wscripti` file — a strict, diff-friendly subset of
wscript syntax (declarations only, think `.d.ts`). A `wscript.toml` next to your scripts
lists the interface files.


```rust
ctx.write_interface("api.wscripti")?;   // from your host's build/setup
```

```toml
# wscript.toml next to your scripts
interfaces = ["api.wscripti"]
```

`wscript check` and `wscript lsp` read the manifest and typecheck scripts against the declared API — completions, hover, and goto-definition included. Regenerate `.wscripti` files in CI to keep them honest. The stdlib ships its own at `wscript-std/wscripti/std.wscripti`.

> [!NOTE]
> **Feature gates grant capability**
> Each stdlib module is a Cargo feature in wscript-std (`math fs process json toml xml`, default all). Registering a module is what grants the capability — don't register `fs`, and scripts cannot touch the filesystem.

## Related

- [Embedding in Rust](../references/concept_embedding.md)

- [Interop: Data vs Opaque Types](../references/concept_interop_types.md)

- [The Type System](../references/concept_type_system.md)

[← Back to SKILL.md](../SKILL.md)
