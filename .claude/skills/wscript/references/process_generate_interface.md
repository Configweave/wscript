# Generate .wscripti and wire up wscript.toml

## Purpose

Give the LSP and `wscript check` knowledge of your host API.

## Prerequisites

- A built Context with your registrations.

## Flowchart

![diagram](../_wdoc/process_generate_interface-diagram-1.svg)

## Steps

### Step 1: Write the interface from the Context

```rust
ctx.write_interface("api.wscripti")?;
```

This dumps every registered signature into a diff-friendly subset of wscript syntax (declarations only). Regenerate it in CI to keep it honest.

### Step 2: List it in wscript.toml

```toml
# wscript.toml next to your scripts
interfaces = ["api.wscripti"]
```

`wscript check` and `wscript lsp` read the manifest and typecheck scripts against the declared API — completions, hover and goto included.

> [!TIP]
> **Verification**
> Opening a script in an LSP-enabled editor gives completions and hover for your host functions; `wscript check` flags misuse of them.

## Related

- [Interface Files & wscript.toml](../references/concept_interface_files.md)

- [The Type System](../references/concept_type_system.md)

[← Back to SKILL.md](../SKILL.md)
