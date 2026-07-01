# Embed wscript in a Rust application

## Purpose

Register a host API, compile a script, and call into it from Rust.

## Prerequisites

- A Rust project depending on the `wscript` umbrella crate.

## Flowchart

![diagram](../_wdoc/process_embed_wscript-diagram-1.svg)

## Steps

### Step 1: Add the dependency

```toml
[dependencies]
wscript = { path = "..." }   # umbrella crate: everything you need
```

The umbrella crate re-exports `Context`, `Module`, `Vm`, `ScriptFn`, `Shared`, and the stdlib modules.

### Step 2: Register host functions in a Module

```rust
use wscript::{Context, Module, Vm};

let mut m = Module::new("term");
m.fn_("print_at", |x: i64, y: i64, s: &str| { /* ... */ });
m.const_("MAX_PANES", 16i64);
```

Any closure whose params are `FromValue + ScriptType` and return is `IntoValue + ScriptType` registers directly (up to 4 params). A host `Err` arrives in the script as `Result::Err`, so `?` composes across the boundary.

### Step 3: Build a Context and compile

```rust
let ctx  = Context::new()
    .module(wscript::std_modules::math())
    .module(m);
let unit = ctx.compile(source)?;   // ← ALL type errors surface here
```

`Context` and `CompiledUnit` are `Send + Sync`. Misuse of a registered API in the script is a compile error here, not a runtime fault.

### Step 4: Run on a per-thread Vm

```rust
let mut vm = Vm::new(&ctx);            // one Vm per thread (!Send)
let n: i64 = vm.call_unit(&unit, "main", ())?;
```

Spin one `Vm` per thread (script values are `Rc`). For hot paths, take a typed handle with `unit.fn_handle::<(A,), R>(name)` — its signature is verified once.

> [!TIP]
> **Verification**
> The script compiles, the Vm call returns the expected value, and calling a registered function with wrong argument types is rejected at compile time.

## Related

- [Embedding in Rust](../references/concept_embedding.md)

- [Interop: Data vs Opaque Types](../references/concept_interop_types.md)

- [Modules & use](../references/concept_modules.md)

[← Back to SKILL.md](../SKILL.md)
