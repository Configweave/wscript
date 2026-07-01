# Workspace crates

| Crate | Contents |
| --- | --- |
| `wscript` | umbrella: embedding API (`Context`, `Vm`, `ScriptFn`, `Shared`), re-exports |
| `wscript-core` | shared types: spans, diagnostics, type repr, bytecode, values, registry |
| `wscript-compiler` | lexer, error-recovering parser, type checker, bytecode emitter |
| `wscript-vm` | register-based VM (one per thread; `Context`/units shared across threads) |
| `wscript-std` | stdlib: math, fs, process, json/toml/xml — capability-style, feature-gated |
| `wscript-macros` | `#[derive(Script)]` |
| `wscript-cli` | the `wscript` binary: `run`, `check`, `repl`, `lsp` |

## Related

- [Embedding in Rust](../references/concept_embedding.md)

[← Back to SKILL.md](../SKILL.md)
