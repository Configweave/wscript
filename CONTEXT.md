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

**Delimited list** — the parser's one shape for `open`, `sep`-separated
elements, `close`: parameter and argument lists, `struct`/`enum` bodies, list
and map literals, closure parameters, patterns. `Parser::list` owns the whole
of it — the separator, trailing separators, newline handling and recovery — so
a call site supplies only its **brackets**, its separator, and what to call the
list in a diagnostic.

**Brackets** — the pair around a **delimited list** (`()`, `[]`, `{}`, `||`).
Everything else is derived from it: the closing token, whether newlines inside
have to be skipped (the lexer suppresses them inside `(` and `[`, but not `{`
or `#{`), and the **follow set**. _Avoid_: restating any of those at a call
site — that drift is what the combinator exists to prevent.

**Follow set** — the tokens a malformed list element resyncs to; one per
**brackets** shape. Outside braces it also carries `{` and `}`, which bound the
block *around* the list: recovery that runs past a block's closing `}` silently
drops every declaration after it.

**Analysis** — the product of running the pipeline up to and including `check`,
without `emit`. What the LSP and `wscript check` consume.

**Index** — the checker's product for the editor: which node sits at a source
offset, and what could be written there. Partial where the lowerings are total —
an editor asks about positions that need not resolve. Built alongside an
**analysis**, on the pipeline's stack. _Avoid_: bare "index" — the **host
index** and the `.wscripti` definition index (`WscriptiIndex`) are unrelated
tables; say which.

**Editor** — the view an editor's questions are asked of: an **index**, the
check tables and the **registry** together, since no question needs fewer than
all three (`Analysis::editor`). `symbol_at` and `completions_at` are what it
answers.

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
internal error rather than a silently-wrong instruction. Patterns lower
differently and have their own space — see **PatLowering**.

**PatLowering** — the same, for one *pattern* node (`check/mod.rs:314`): the
variant or struct it selects and the runtime index of each field as written,
or a unit literal's folded constant. Read it through
`CheckResult::pat_lowering`. Patterns needed a separate space because they
test and bind rather than produce a value, and they needed a separate fold
because a pattern's identity and its field order were recorded by different
functions: `check_pattern_fields` is shared by struct patterns and
struct-variant patterns, so it *returns* the order and each caller writes one
complete value.

**Operand** — the descriptor the operator ladders decide over (`check/ops.rs:135`):
a **shape** (`Int`, `Quantity`, `Named`, `Container`, `Poison`, …), the user
`impl` of this operator's trait if there is one, and whether the type supports
the operation **structurally** — derives it, or its elements do, or the type
parameter declares the bound. Three fields serve all four ladders because the
checker's **shell** — the effectful methods wrapped around the pure ladder
table — resolves the operator-specific question before building the descriptor.

**Diagnostic** — a compile-time message carrying a stable `code` (`E0001`…),
severity, labelled spans and optional help.

**Code registry** — `CODES` in `diag.rs`: the canonical list of every
diagnostic code, its fallback help, and whether the fixture corpus covers it or
is **exempt** with a recorded reason. Every code carries help, so no diagnostic
can render mute whichever site raised it — that is how "every error explains
itself" is enforced rather than hoped for. **Site help** (`error_help`,
`with_help`, `ops::Msg`) wins where it exists, because it can name the type or
argument that actually went wrong. The gates live in
`wscript-compiler/tests/diag_codes.rs` (registry vs. source) and
`diag_snapshots.rs` (registry vs. corpus). _Avoid_: bare "registry" — that is
the host-registration **Registry**, an unrelated type the same tests also use.

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

**Scratch** — a register the emitter allocates for an intermediate value, handed
to a scope and released when that scope ends (`emit/code.rs`). Locals and capture
cells sit at fixed slots; everything above them is scratch, so a frame is sized
by the body's deepest expression rather than by the sum of its expressions.
_Avoid_: "temporary variable" — a scratch names no binding.

**Value register** (`ValueReg`) — where an emitted expression's value landed, and
whether the emitter owns it: a **scratch** it allocated, or a register the frame
owns (a local, a capture cell) that it may read but not clobber. Reading a plain
local borrows its register instead of copying it, so this is what tells the two
apart. _Avoid_: "operand" — that is the checker's operator-ladder descriptor
above, an unrelated type in the same crate.

**Label** — a branch target created before its position is known, bound exactly
once, and resolved to an offset when the body is finished; a body holding a label
that was never bound does not finish. "Patching" a jump is the mechanism
underneath — private to the label table, and no longer how the emitter reasons.

**Verification** — the check that a compiled unit's operands are in range: every
register below the proto's `n_regs`, every jump inside its body, every constant /
def / proto / vtable index within its table (`wscript-core/src/verify.rs`). It is
a compile-side assertion held over the script corpus by the test suite, *not* a
load-time gate: the VM still indexes with these operands unchecked, and an
embedder calling `compile` pays nothing for it.

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
guarantee; it exists only in memory between `compile` and `run`. It is also
**trusted**: the dispatch loop indexes with instruction operands directly rather
than re-validating them, so running a hand-crafted or corrupted unit may panic.
Everything crossing the *script* boundary is checked and faults instead.

---

## Runtime

**Frame** — one activation record: a base into the register stack, a proto, a
program counter and a return slot.

**Control transfer** — anything that moves a frame's program counter other than
stepping to the next instruction: a taken jump, a call into a frame, a return
out of one, and the escape into native code (host function or builtin) that can
spend fuel without moving the pc at all. The unit **fuel** is charged in: each
transfer pays for the straight-line run of instructions that ends at it.

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
and every VM spun from the owning `Context`. _Avoid_: using it for the
diagnostic **code registry**, which is a different table entirely.

**Module** — a namespace of host functions, consts and types, registered by the
embedder and reachable from script as `name::thing`.

**Host index** — the number a resolved host call carries: a position in the
registry's `host_fns`, where the implementation lives. `Registry::host_ref`
maps it back to the declaration that minted it — module or type, name,
signature, docs — which is how an editor names a host symbol. _Avoid_: bare
"index", which is the editor's **index**.

**Host function** — a Rust function callable from script. Registered once;
consumed by the checker (signature), the emitter (call target) and the VM
(dispatch) with no per-function code in any of them.

**Declared parameter name** — the name a host gives a registered parameter
(`fn_named`, `method_named`). Documentation, not type structure: it lives beside
the `FnSig` in the registry entry, never inside it, because `FnSig` is part of
type identity. Where nothing was declared the parameter is **positional** and
shows as the placeholder `a0`, `a1`, … — never an invented name.

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
directives inside the fixture. Every file directly in that directory is an
**entry**; a file that exists only to be `use`d by one of them is a **module
fixture** and lives in `tests/scripts/modules/`, where nothing runs it.
