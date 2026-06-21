# CLAUDE.md

Project context for Claude Code.

## Project Purpose

**wscript** is a scripting language implemented in Rust. The product
requirements live in `docs/PRD.md` — read that first; it is the source of
truth for language design and scope.

## Status

v1 complete: all seven PRD milestones (M1–M7) are implemented and tested —
core pipeline, data types, traits/Rc semantics, Rust interop, stdlib,
tooling (REPL/LSP/wscript.toml), and polish (diagnostics/docs/benchmarks).

## Layout

Cargo workspace per PRD §2: `wscript` (umbrella/embedding API), `wscript-core`
(shared types/bytecode/values), `wscript-compiler`, `wscript-vm`, `wscript-std`,
`wscript-macros` (`#[derive(Script)]`), `wscript-cli` (binary `wscript`),
`benchmarks`. Script behaviour tests live in `tests/scripts/*.wscript`
(asserted by `wscript-cli/tests/scripts.rs` via `// expect:` directives).
Docs: `docs/tour.md`, `docs/embedding.md`, `docs/stdlib.md`,
`docs/benchmarks.md`.

## Conventions

- Trunk-based development: commit directly to `main`, no branches or PRs
  unless explicitly asked.
- Standard Rust toolchain: `cargo build`, `cargo test`, `cargo clippy`,
  `cargo fmt`.
- Regenerate the stdlib interface after changing registrations:
  `WSCRIPT_REGEN_WSCRIPTI=1 cargo test -p wscript-cli --test wscripti_gen`.
