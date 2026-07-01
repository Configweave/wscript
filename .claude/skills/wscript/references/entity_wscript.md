# wscript

_tool_

An embeddable, statically typed, Rust-flavored scripting language with a bytecode compiler, a register VM, and first-class Rust interop.

wscript is a scripting language that embeds in Rust applications the way Lua embeds in C. It compiles to bytecode and runs on a register-based VM; the surface is Rust with the borrow checker, lifetimes and generics removed. Memory is pure reference counting, so aliased mutation is free — the Lua/Python mental model. Its defining trait is compile-time type safety across the host boundary: exposing a Rust API is one `#[derive(Script)]` and one registration call, and misuse of it in a script is a \*compile\* error. The `wscript` binary runs, checks, REPLs and language-servers scripts.

| Field | Value |
| --- | --- |
| Kind | Embeddable scripting language (Rust host) |
| Binary | `wscript` — `run` · `check` · `repl` · `lsp` |
| Built in | Rust (workspace of crates: compiler, vm, std, macros, cli) |
| Source | [github.com/wiltaylor/wscript](https://github.com/wiltaylor/wscript) |
| Status | v1 complete (milestones M1–M7) |

## Related

- [Embedding in Rust](../references/concept_embedding.md)

- [The Type System](../references/concept_type_system.md)

- [The `wscript` CLI](../references/fact_cli_commands.md)

[← Back to SKILL.md](../SKILL.md)
