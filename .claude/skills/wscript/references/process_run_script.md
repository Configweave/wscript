# Run a wscript script

## Purpose

Compile and execute a .wscript file with the full standard library.

## Prerequisites

- The wscript binary is built (`cargo build --release`) or installed on PATH.

## Flowchart

![diagram](../_wdoc/process_run_script-diagram-1.svg)

## Steps

### Step 1: Write a script with a main function

```rust
fn main() -> int {
    println("hello")
    0
}
```

Execution starts at `fn main()`. Its `int` return is the exit code; `unit` means 0.

### Step 2: Run it

```console
wscript run hello.wscript
```

Pass any script arguments after the path — read them with `process::args()`.

> [!TIP]
> **Verification**
> The script's output appears; a fault (if any) prints with a source snippet and stack trace, and the exit code reflects `main`'s return.

## Related

- [Faults](../references/concept_faults.md)

- [Modules & use](../references/concept_modules.md)

[← Back to SKILL.md](../SKILL.md)
