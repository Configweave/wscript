---
name: wscript
description: "Reference and processes for wscript — an embeddable, statically typed, Rust-flavored scripting language: bytecode-compiled, register-VM, with first-class Rust interop and reference-counted (no borrow-checker) semantics. Use when writing, checking, running or embedding wscript, or answering questions about its language, standard library, the `wscript` CLI, or the Rust embedding API."
wskill_schema_version: 1.1.0
allowed-tools: []
disallowed-tools: []
disable-model-invocation: false
---

# wscript

An embeddable, statically typed, Rust-flavored scripting language: bytecode-compiled, register-VM, with first-class Rust interop and reference-counted (no borrow-checker) semantics.

**Upstream version:** `0.1.0`. If the real upstream has moved past this, the skill may be stale — bump `topic.version` and re-verify (see the update workflow).

wscript is an embeddable, statically typed, Rust-flavored scripting language. This skill captures its full reference as data — the language, the standard library, the `wscript` CLI, and the Rust embedding API — projected from one model.

## Parameters

Values to pass when invoking this skill — reference them as `$ARGUMENTS`, `$1`, `$2`, … in the prompt.

| Parameter | Description | How to determine the value |
| --- | --- | --- |
| $ARGUMENTS | The wscript topic, stdlib function, CLI subcommand, or embedding API to look up. | Take it from the user's request — e.g. the language feature, method, module, or `wscript` subcommand they asked about. If empty, summarise the reference and ask what they need. |
| $1 | Optional area to scope the answer to: `language`, `stdlib`, `cli`, or `embedding`. | Infer from the question; default to searching all areas when unset. |

<Boundary>

**Always:**

- Cite the exact reference page when answering.

- Prefer the documented language/stdlib/CLI form over guesses.

- Match wscript syntax exactly — no implicit numeric conversion, no truthiness, `self` is by reference (no `&`), map literals are `#{...}`, closures are `|x| ...`.

**Ask first:**

- Before running a script that touches the filesystem (`fs`) or spawns processes (`process`).

**Never:**

- Invent stdlib functions, methods, CLI flags, or syntax that aren't in the reference.

- Suggest v1-unsupported features (borrow checker, generics, `+=`, bitwise ops, async, threads-per-value, string interpolation) as if they exist.

</Boundary>

## Reference

### Getting started

_What wscript is, the `wscript` CLI, and the first runbooks._

## What wscript is

An embeddable, statically typed, Rust-flavored scripting language. It compiles to bytecode and runs on a register-based VM, with first-class Rust interop and reference-counted (no borrow-checker) semantics — the niche Lua fills for C, for Rust.

## Build

Standard Rust toolchain. The binary lands at `target/release/wscript`.

```console
cargo build --release
```

## Write a script

Execution starts at `fn main()`; its `int` return becomes the exit code (`unit` means 0). The file extension is `.wscript`.

```rust
// hello.wscript
fn main() -> int {
    println(fmt("Hello, {}!", "wscript"))
    0
}
```

## Run it

| Command | Does |
| --- | --- |
| `wscript run hello.wscript [args…]` | compile + execute with the full stdlib |
| `wscript check hello.wscript` | compile only; honors `wscript.toml` / `.wscripti` |
| `wscript repl` | persistent-binding REPL |
| `wscript lsp` | language server (diagnostics, hover, goto, completions) |

## Embed in Rust (five steps)

1. Register your API in a `Module`. 2. Build a `Context` from your modules plus any stdlib modules. 3. `ctx.compile(source)` once — all type errors surface here. 4. Spin one `Vm` per thread. 5. Call script functions with `vm.call` / `unit.fn_handle`.

See the [Embedding](references/concept_embedding.md) concept and the **Embed wscript in a Rust application** process for the full story.

- [wscript](references/entity_wscript.md)

- [The `wscript` CLI](references/fact_cli_commands.md)

- [Run a wscript script](references/process_run_script.md)

- [Type-check a script (with host APIs)](references/process_check_script.md)

- [Explore wscript in the REPL](references/process_use_repl.md)

### Language Basics

_Values, strings, functions and control flow._

- [Values & Types](references/concept_values.md)

- [Primitive types](references/fact_primitive_types.md)

- [Strings](references/concept_strings.md)

- [string methods](references/fact_string_methods.md)

- [Functions & Closures](references/concept_functions.md)

- [Control Flow](references/concept_control_flow.md)

### Data Types

_Structs, enums, pattern matching, error handling and containers._

- [Structs & Methods](references/concept_structs.md)

- [Enums](references/concept_enums.md)

- [Pattern Matching](references/concept_pattern_matching.md)

- [Option, Result & ?](references/concept_options_results.md)

- [Option & Result methods](references/fact_option_result_methods.md)

- [Containers: List & Map](references/concept_containers.md)

- \[List\[T\] methods\](fact_list_methods)

- \[Map\[K, V\] methods\](fact_map_methods)

### Traits & Operators

_Interfaces, operator overloading and derives._

- [Traits](references/concept_traits.md)

- [Operator Overloading](references/concept_operators.md)

- [Operator overloading traits](references/fact_operator_traits.md)

- [Derives](references/concept_derives.md)

### Memory & References

_Reference semantics, reference counting and faults._

- [Reference Semantics](references/concept_reference_semantics.md)

- [Reference types](references/fact_reference_types.md)

- \[Memory: Reference Counting & weak\[T\]\](concept_memory)

- [Faults](references/concept_faults.md)

### Modules & Type System

_Importing host APIs, and the static type system._

- [Modules & use](references/concept_modules.md)

- [The Type System](references/concept_type_system.md)

- [Not in wscript v1 (by design)](references/fact_not_in_v1.md)

### Standard Library

_The prelude plus the capability-style stdlib modules._

- [Prelude (always available, no import)](references/fact_prelude_functions.md)

- [math module](references/fact_math_functions.md)

- [fs module](references/fact_fs_functions.md)

- [process module](references/fact_process_functions.md)

- [json / toml / xml modules](references/fact_data_formats.md)

- [The shared Value type](references/fact_value_methods.md)

### Embedding (Rust interop)

_Exposing host functions and types to scripts, checked at compile time._

- [Embedding in Rust](references/concept_embedding.md)

- [Workspace crates](references/fact_crates.md)

- [Interop: Data vs Opaque Types](references/concept_interop_types.md)

- [Interface Files & wscript.toml](references/concept_interface_files.md)

- [Embed wscript in a Rust application](references/process_embed_wscript.md)

- [Expose a Rust type to scripts](references/process_expose_host_type.md)

- [Generate .wscripti and wire up wscript.toml](references/process_generate_interface.md)

### Project & People

_How wscript was built, the tooling it rests on, and who maintains it._

- [Milestones (M1-M7)](references/fact_milestones.md)

- [WCL](references/entity_wcl.md)
