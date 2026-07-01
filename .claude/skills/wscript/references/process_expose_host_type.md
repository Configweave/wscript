# Expose a Rust type to scripts

## Purpose

Make a Rust struct or enum constructible (data) or method-only (opaque) from a script.

## Prerequisites

- You are registering modules into a Context (see the embed runbook).

## Flowchart

![diagram](../_wdoc/process_expose_host_type-diagram-1.svg)

## Steps

### Step 1: Derive Script for a data type

```rust
#[derive(Script)]
struct Point { x: i64, y: i64 }    // fields visible; constructible & matchable
```

Data types cross by value: the script constructs a `Point`, your function receives a real one. Enums derive too, payloads included.

### Step 2: Or make it opaque (handle, methods only)

```rust
#[derive(Script)]
#[script(opaque)]
struct Pane { title: String }

m.ty::<Pane>()
    .method("title", |p: &Pane| p.title.clone())
    .method("split", |p: &mut Pane, vertical: bool| { /* ... */ });
```

Opaque types cross by handle: `&mut` methods mutate the live Rust value in place; scripts cannot read fields or construct it.

### Step 3: Register types used only at boundaries

```rust
let ctx = Context::new().register_type::<KeyEvent>();
```

A type that appears in no module (e.g. only in a `ScriptFn` signature) still needs registering. Registered types are ambient — `use` only governs functions and constants.

> [!TIP]
> **Verification**
> The script can construct/match data types and call methods on opaque handles; field access on an opaque type is a compile error.

## Related

- [Interop: Data vs Opaque Types](../references/concept_interop_types.md)

- [Embedding in Rust](../references/concept_embedding.md)

[← Back to SKILL.md](../SKILL.md)
