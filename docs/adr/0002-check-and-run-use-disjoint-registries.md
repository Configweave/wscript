# ADR-0002: `check` and `run` use disjoint registries when a manifest is present

- **Status:** accepted (pre-existing behaviour, recorded)
- **Date:** 2026-08-01
- **Context:** [#3](https://github.com/Configweave/wscript/issues/3) (deepening programme), raised while designing [#13](https://github.com/Configweave/wscript/issues/13)

## Context

`wscript run` builds its registry from the CLI's bundled stdlib
(`wscript-cli/src/main.rs:55-61`). `wscript check` does not, **when a
`wscript.toml` is found** (`main.rs:132-139`):

```rust
let ctx = match manifest::find(std::path::Path::new(path)) {
    Some(m) => {
        let mut reg = wscript::Registry::new();     // <- no stdlib
        manifest::load_interfaces(&m, &mut reg);
        wscript::Context::from_registry(reg)
    }
    None => default_context(Vec::new()),            // <- stdlib, same as run
};
```

So the two commands can resolve the same script against different host contexts,
and can therefore disagree. With no manifest they agree.

This surfaced during the design of a `Session` module that unifies pipeline
wiring across `run`, `check`, the REPL, the LSP and benchmarks, where the
obvious simplification is to give every caller one registry.

## Decision

**Preserve the behaviour**, expressed as an explicit `Session` mode
(`Mode::Run` — stdlib plus source roots; `Mode::Check` — manifest interfaces plus
source roots) rather than as a re-derivation at each call site.

Do **not** unify the two registries.

## Rationale

A manifest describes the *complete* host context a script runs under — interface
dumps include everything the embedder registers. If the CLI's own stdlib stayed
in the registry alongside those interfaces, a same-named CLI module would
**shadow the embedder's** one. A script written against an embedder's `fs` would
then be checked against the CLI's `fs`, and mis-checked: passing where it should
fail, or failing where it should pass.

The stdlib is not privileged here. An embedder that wants it checks against it by
listing the stdlib's own `.wscripti` in the manifest — which is exactly what
`wscript-std/wscripti/std.wscripti` is generated for.

Unifying the registries would trade a visible, explainable difference between two
commands for a silent, hard-to-diagnose wrong answer inside one of them.

## Consequences

- `wscript check` and `wscript run` may disagree for a project with a manifest.
  This is intended; the fix for a surprising disagreement is to list the missing
  interface in `wscript.toml`, not to change the registry rule.
- The LSP must use `Mode::Check`, so an editor agrees with `wscript check` rather
  than with `wscript run`.
- The modes are a seam worth testing directly. As of this ADR **no `wscript.toml`
  exists anywhere in the repo**, so neither mode is exercised; a fixture is in
  scope for [#13](https://github.com/Configweave/wscript/issues/13).
- Anyone tempted to simplify this — including a future architecture review —
  should read this file first.
