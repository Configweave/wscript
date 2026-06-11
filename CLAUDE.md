# CLAUDE.md

Project context for Claude Code.

## Project Purpose

**wisp** is a scripting language implemented in Rust. The product
requirements live in `docs/PRD.md` — read that first; it is the source of
truth for language design and scope.

## Status

Scaffold only. Structure (crates, modules) will be shaped by the PRD once
it lands.

## Conventions

- Trunk-based development: commit directly to `main`, no branches or PRs
  unless explicitly asked.
- Standard Rust toolchain: `cargo build`, `cargo test`, `cargo clippy`,
  `cargo fmt`.
