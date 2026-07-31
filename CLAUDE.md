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
Docs are authored in wdoc/WCL: `docs/main.wcl` is the landing site and
`docs/wskills/wscript/` the reference model (language, stdlib, CLI, embedding),
projected into a book, the committed `.claude/skills/wscript` skill, a deck and
a training course. `just docs-serve` previews; `just skill-build` regenerates
the skill (delete `.claude/skills/wscript` first — stale files aren't wiped).

## Conventions

- Ticket-branch development, driven by the aciddog kanban board: work happens
  on a branch named for the ticket id (`t-…`) in that ticket's worktree at
  `.tree/<ticket-id>`, and lands on `main` through a pull request. Never commit
  or push directly to `main` — the board's Tests and Review stages gate every
  change, and a direct push bypasses them.
- Standard Rust toolchain: `cargo build`, `cargo test`, `cargo clippy`,
  `cargo fmt`.
- `just ci::check` is the merge bar — format check, lint, tests, examples. It
  lives in `.just/ci/mod.just`; its recipes are in `.just/shared.just` so both
  the module and the root justfile can import them. `.github/workflows/ci.yml`
  fans the same recipes out one per job, so the two cannot drift.
- Regenerate the stdlib interface after changing registrations:
  `WSCRIPT_REGEN_WSCRIPTI=1 cargo test -p wscript-cli --test wscripti_gen`.

## Agent skills

### Issue tracker

Issues live in GitHub Issues for `Configweave/wscript`, driven via the `gh`
CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

The five canonical roles, each label string equal to its name. See
`docs/agents/triage-labels.md`.

### Domain docs

Single-context: `CONTEXT.md` and `docs/adr/` at the repo root. See
`docs/agents/domain.md`.
