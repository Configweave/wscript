# Explore wscript in the REPL

## Purpose

Try expressions interactively with bindings that persist across lines.

## Prerequisites

- The wscript binary is available.

## Flowchart

![diagram](../_wdoc/process_use_repl-diagram-1.svg)

## Steps

### Step 1: Start the REPL

```console
wscript repl
```

Bindings you create persist for the session.

### Step 2: Evaluate expressions

```rust
let xs = [1, 2, 3]
xs.map(|x| x * 2).fold(0, |a, x| a + x)   // 12
```

The prelude (`print println str fmt same weak int float`) is always available; `use math` etc. brings in stdlib modules.

> [!TIP]
> **Verification**
> Expressions evaluate and earlier `let` bindings remain in scope on later lines.

## Related

- [Modules & use](../references/concept_modules.md)

[← Back to SKILL.md](../SKILL.md)
