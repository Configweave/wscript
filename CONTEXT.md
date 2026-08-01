# CONTEXT

The domain language of **wscript** — an embeddable, statically typed scripting
language implemented in Rust: bytecode-compiled, register-VM, with first-class
Rust interop and reference-counted (no borrow-checker) semantics.

Use these terms as defined here. Where a term has a tempting synonym that means
something else in this codebase, the synonym is listed under _Avoid_.

`docs/PRD.md` is the source of truth for language design and scope; this file
names the things the implementation is built from. Architectural decisions live
in `docs/adr/`.

---

## The pipeline

**Source file** — one `.wscript` file. Multi-file programs are reached by
script-to-script `use` imports from an **entry**; all files in one compilation
share a single span address space and a single `NodeId` counter.

**Span address space** — spans across a multi-file compilation are rebased so
each file occupies a disjoint range, with a one-byte gap between files. A
`SourceMap` maps a span back to `(file, offset)`.

**NodeId** — the join key between the AST and everything the checker learned.
Unique across all files in a compilation. _Avoid_: "node index" — ids are not
positions.

**Stages** — lex → parse → check → emit → run. `check` and `emit` are separate
crates from the VM; `wscript` (the umbrella) is the embedding API.

**Analysis** — the product of running the pipeline up to and including `check`,
without `emit`. What the LSP and `wscript check` consume.

---

## Compile-time model

**Def** — a named, nominal definition: a struct, enum, trait or unit family
(`DefKind`, `wscript-core/src/defs.rs:37`). Lives in the **DefTable**, keyed by
`DefId`. Host registrations and script declarations share one table.

**Unit family** — a nominal numeric type backed by `int` or `float` whose values
are stored normalised to a **base unit** (PRD §3.10, `DefKind::Unit`). A value of
a unit family is called a **quantity**; `DefTable::is_quantity` is the predicate.
Each unit in the family has a **factor** (`Factor::Int` / `Factor::Float`) giving
its conversion to the base. Quantities are zero-cost — they lower to plain
numbers. _Avoid_: "dimension" (wscript does not model dimensional algebra —
multiplying two quantities is an error, not a new dimension); "newtype".

**Prelude** — the eight functions always in scope and lowered VM-natively rather
than called: `print`, `println`, `str`, `fmt`, `same`, `weak`, `int`, `float`
(`PreludeFn`, `check/mod.rs:54`). _Avoid_: "stdlib" — the stdlib is host modules,
registered and callable, and is a different thing.

**Local** — a binding in a function frame, identified by a `LocalId`. A `LocalId`
is a **frame slot index** and is never reused across sibling scopes; see
[ADR-0001](docs/adr/0001-localid-is-a-frame-slot.md).

**Capture** — a local of an enclosing function referenced from inside a closure.
The owning function's local is **cell-allocated** (boxed) so the closure can
share it, and capture slots are threaded through every intervening closure.

> **Hazard:** two near-identical types describe capture sources.
> `CapSrc` (`check/mod.rs:196`) is the *checker's* view — `Local(LocalId)` or
> `Capture(u16)`. `CaptureSrc` (`bytecode.rs:508`) is the *bytecode's* view —
> `Reg(u16)` or `Capture(u16)`. They are not interchangeable.

**Env** — the checker's body-checking state: lexical scopes, function frames,
loops and the type parameters currently in scope (`check/env.rs:59`). Plain data
holding no reference to the checker, so capture threading — the subtlest logic
in it — is unit-testable on its own. Callers enter through scoped methods on
`Checker` (`in_scope`, `in_fn`, `in_loop`, `with_type_params`) rather than the
push/pop pairs beneath them.

**Lowering** — how the checker decided one *expression* node becomes bytecode
(`check/mod.rs:286`). One per node, replacing thirteen parallel
`HashMap<NodeId, _>` side tables; payloads that used to need a second lookup — a
unit conversion's factor, a struct literal's field permutation — are inline in
the variant. Read it through `CheckResult::lowering`, so a missing entry is an
internal error rather than a silently-wrong instruction. Patterns are not in
this space; they keep their own tables.

**Operand** — the descriptor the operator ladders decide over (`check/ops.rs:135`):
a **shape** (`Int`, `Quantity`, `Named`, `Container`, `Poison`, …), the user
`impl` of this operator's trait if there is one, and whether the type supports
the operation **structurally** — derives it, or its elements do, or the type
parameter declares the bound. Three fields serve all four ladders because the
caller resolves the operator-specific question before building the descriptor.

**Diagnostic** — a compile-time message carrying a stable `code` (`E0001`…),
severity, labelled spans and optional help. Every code should explain itself;
`diag.rs` holds fallback help text for codes whose sites supply none.

**Interface file** (`.wscripti`) — a declaration-only file describing host
modules, functions, consts and opaque types, so `wscript check` and the LSP can
typecheck against a host they cannot link. Generated from a `Registry`, consumed
by the checker. _Avoid_: "header", "stub".

---

## Bytecode

**Compiled unit** — the output of compilation and everything a VM needs to run:
protos, constants, def table, vtables, impl maps. `Send + Sync` by construction
(no `Rc`), so one compilation can feed many per-thread VMs.

**Proto** (`FnProto`) — a compiled function body: code, a span per instruction,
parameter count, total register-file size (`n_regs`) and its capture sources.
_Avoid_: "chunk", "function object" (that is a runtime closure value).

**Register** — a slot in a frame's register file. Instructions name registers
directly. Arguments occupy registers `0..n_params`; calls pass arguments in a
contiguous **window** `base..base + nargs`.

**Vtable** — the method table for one `(concrete type, trait)` impl, used by
`dyn Trait` dispatch; slot order follows the trait's declaration order.

**Impl maps** — custom (non-derived) `Eq` / `Ord` / `Display` impls, consulted by
the runtime's structural equality, ordering and display routines when they
descend into nested values.

**Builtin** — a method implemented natively by the VM rather than compiled
(`Builtin`, `bytecode.rs:54`); the receiver's methods on `str`, `list`, `map`,
`option`, `result` and `weak`. Its *type* is declared in the checker's scheme
table and its *implementation* in the VM, connected only by the variant name.

The bytecode format is **internal and unstable** — there is no serialization
guarantee; it exists only in memory between `compile` and `run`.

---

## Runtime

**Frame** — one activation record: a base into the register stack, a proto, a
program counter and a return slot.

**Fuel** — an execution budget in dispatched instructions, bounding runaway
scripts deterministically. The tank belongs to the VM and depletes across calls.
Charged at control-transfer points rather than per instruction, so exhaustion
surfaces at the end of the current straight-line run. Structural operations
(deep equality, clone, display) additionally charge per value visited.

**Fault** — a runtime error (`ScriptFault`): a message, a span and a **trace** of
frames. Trappable by the host. Distinct from a **diagnostic**, which is
compile-time and carries a code.

**Cell** — a heap box holding a captured local, so a closure and its enclosing
frame share one mutable binding.

**Shared / weak** — reference-counted script values. `weak` breaks cycles;
`same(a, b)` is reference identity, distinct from `==`.

---

## Host interop

**Registry** — every host registration visible to a compilation: defs, modules,
host functions and methods of host types. Shared immutably between the checker
and every VM spun from the owning `Context`.

**Module** — a namespace of host functions, consts and types, registered by the
embedder and reachable from script as `name::thing`.

**Host function** — a Rust function callable from script. Registered once;
consumed by the checker (signature), the emitter (call target) and the VM
(dispatch) with no per-function code in any of them.

**Opaque type** — a host Rust type exposed to script as a nominal type with
methods but no visible fields.

**Context** — the embedder's handle to a configured registry, from which
compilations and VMs are made.

**Session** — a registry **and** the resolver its imports go through, wired once
and kept together. Answers "how is this project compiled?" where a `Context`
answers only "what has the host registered?". Cheap to clone; `analyze`,
`compile` and `run` hang off it. Every tool — `run`, `check`, the REPL, the LSP,
the benchmarks — builds one instead of reassembling the pipeline.

**Resolver** (`SourceResolver`) — how a `use` is turned into source text.
`FsResolver` searches the importing file's directory then the **source roots**;
`NoImports` refuses everything; a host may supply its own.

---

## Tooling

**Manifest** (`wscript.toml`) — project configuration: source roots for import
resolution, and interface files to typecheck against.

**Source roots** — directories an import is resolved against.

**Project** — what the CLI derives from a path on disk: the **session** to
compile with, plus the `.wscripti` indexes for goto-definition. Built by
`project_for(entry, mode)`, the one place that turns a file into a configured
compiler.

**Mode** — which host a project is compiled against. `Run`: the CLI's stdlib,
plus the script's arguments. `Check`: the manifest's declared interfaces instead
of the stdlib (ADR-0002). Source roots come from the manifest in both — that is
not a mode difference, which is why the LSP can no longer drop them.

**Script suite** — the behaviour tests in `tests/scripts/*.wscript`, asserted by
`// expect:` (stdout), `// exit:` (exit code) and `// error:` (rendered stderr)
directives inside the fixture.

---

## Planned terms

Named by the deepening programme in
[#3](https://github.com/Configweave/wscript/issues/3) and **not yet in the code**.
Listed here so the tickets and the glossary agree. `Lowering`, `Operand` and
`Env` have since shipped and are defined above.

**Index** — the checker's product for the editor: `symbol_at`, `completions_at`,
`span_of`, `methods_of`. Partial where the lowerings are total — an editor asks
about positions that need not resolve. The last entry here; it retires this
section when [#16](https://github.com/Configweave/wscript/issues/16) lands.
