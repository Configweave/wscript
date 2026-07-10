# Embedding wscript in a Rust application

wscript's reason to exist is PRD §6: exposing host functions and types is one
derive and one registration call, with plain Rust signatures, and **all**
type errors — including misuse of your host API — surface when the script
compiles, not when it runs.

```toml
[dependencies]
wscript = { path = "..." }          # umbrella crate: everything you need
```

## The 60-second version

```rust
use wscript::{Context, Module, Vm};

let mut m = Module::new("term");
m.fn_("clear", || { /* ... */ });
m.fn_("print_at", |x: i64, y: i64, s: &str| { /* ... */ });
m.const_("MAX_PANES", 16i64);

let ctx = Context::new()
    .module(wscript::std_modules::math())
    .module(m);

let unit = ctx.compile(r#"
    use term
    use math
    fn main() -> int {
        term::print_at(1, 2, "hello")
        term::MAX_PANES
    }
"#)?;                                   // ← ALL type errors surface here

let mut vm = Vm::new(&ctx);
let n: i64 = vm.call_unit(&unit, "main", ())?;
```

Calling `term::print_at("oops", 1, 2)` from the script is a *compile*
error with a span and a help message, because registration captured the
signature for the checker.

## The pieces

- **`Context`** — host registrations + compiler. `Send + Sync` and
  cheaply cloneable: compile once, share the `Context` and the
  `CompiledUnit` across threads, and spin one `Vm` per thread.
- **`Module`** — a named bag of functions, constants and types. Scripts
  opt in with `use name`. Nothing is ambient (capability-style): a script
  can only touch what you registered.
- **`Vm`** — per-thread execution. `!Send` by construction (script values
  are `Rc`-managed).
- **`CompiledUnit`** — the output of `ctx.compile(source)`. Send/Sync.

## Registering functions

Any closure whose parameters implement `FromValue + ScriptType` and whose
return implements `IntoValue + ScriptType` registers directly:

```rust
m.fn_("rename", |id: i64, name: &str| -> bool { ... });
m.fn_("read_key", || -> Result<char, String> { ... });
m.fn_("tags", || -> Vec<String> { ... });
m.fn_("lookup", |k: String| -> Option<i64> { ... });
```

Supported parameter/return types: `i64` (and narrower ints), `f64`/`f32`,
`bool`, `char`, `()`, `String`, `&str` (params, any position up to 4
parameters), `Vec<T>`, `HashMap<K, V>`, `Option<T>`, `Result<T, E>`, and
any `#[derive(Script)]` type.

A host `Err` arrives in the script as `Result::Err`, so `?` composes
across the boundary. The error type must itself be convertible (use
`String`, or a derived type): `Result<T, String>` is the common shape.

Attach docs for the LSP with `m.doc_next("...")` before a registration.

## Exposing types

```rust
#[derive(Script)]                 // DATA type: fields visible in script,
struct Point { x: i64, y: i64 }   // constructible and matchable

#[derive(Script)]
enum Event {                      // enums derive too, incl. payloads
    Quit,
    Key(char),
    Click { x: i64, y: i64 },
}

#[derive(Script)]
#[script(opaque)]                 // HANDLE type: no fields, methods only
struct Pane { title: String }

m.ty::<Pane>()
    .method("title", |p: &Pane| p.title.clone())
    .method("split", |p: &mut Pane, vertical: bool| { ... });
```

- **Data types convert by value**: the script constructs a `Point`, your
  function receives a real `Point`. Mutations inside the script to its
  copy do not write back into yours.
- **Opaque types cross by handle**: the script holds a reference to the
  live Rust value; `&mut` methods mutate it in place. Scripts cannot
  touch fields or construct opaque values.
- Types are **ambient** in the script's type namespace once registered —
  `use` only governs functions and constants.
- A type that appears in no module (e.g. only in `ScriptFn` boundaries)
  still needs registering: `Context::new().register_type::<KeyEvent>()`.

Options: `#[script(name = "Other")]` renames the script-side type;
`#[script(crate_path = "wscript_core")]` for crates that depend on
`wscript-core` directly.

## Calling script from Rust

```rust
// One-shot, types checked at the boundary:
let n: i64 = vm.call(&unit, "compute", (5i64, "scale"))?;

// Hot path — typed handle, signature verified once at lookup:
use wscript::UnitExt;
let on_key: wscript::ScriptFn<(KeyEvent,), bool> = unit.fn_handle("on_key")?;
let quit = on_key.call(&mut vm, (key_event,))?;   // cheap thereafter
```

`fn_handle` fails with `Error::Signature` if the script function's
signature doesn't match `(A...) -> R`. Generic script fns (`fn f[T]`)
are not callable from the host — wrap them in a monomorphic script fn.

Scripts that import other script files compile through
`ctx.compile_entry(path, src, &resolver)` — `FsResolver` resolves
imports on disk (plain `ctx.compile` refuses them with a pointed
error). The whole import graph still produces ONE `CompiledUnit`;
`unit.source_map` plus the returned per-file sources let you render
diagnostics and fault traces against the right file. Implement
`SourceResolver` yourself to load imports from memory, an archive, or
anywhere else.

## Script callbacks (host functions taking closures)

A host function can receive a script closure and invoke it: declare a
`ScriptClosure<Args, Ret>` parameter and take `&mut dyn HostCtx` as the
*first* Rust parameter (it does not appear in the script-visible
signature):

```rust
use wscript::{Fault, HostCtx, ScriptClosure};

m.fn_(
    "retry",
    |ctx: &mut dyn HostCtx, cb: ScriptClosure<(i64,), bool>, times: i64| -> Fault<bool> {
        for i in 0..times {
            match cb.call(ctx, (i,)) {
                Ok(true) => return Fault(Ok(true)),
                Ok(false) => continue,
                Err(e) => return Fault(Err(e)),   // propagate script faults
            }
        }
        Fault(Ok(false))
    },
);
```

Scripts see `fn retry(fn(int) -> bool, int) -> bool` — passing a
mis-typed closure is a **compile-time** error like any other host
signature misuse. What you get:

- **Faults compose.** A fault inside the callback arrives as a
  `HostError` whose `.fault` carries the callback's stack trace.
  Propagate it (as above) and the embedder sees one coherent trace:
  callback frames → `<host function>` → outer script frames. Or match
  on it and recover — the VM unwinds the callback's frames either way.
- **Fuel composes.** Callback instructions draw from the same tank as
  the rest of the script; a runaway callback faults with "fuel
  exhausted" like any other code.
- **Ping-pong is bounded.** Host→script re-entries nest at most
  `REENTRY_DEPTH_LIMIT` (32) deep — mutual host/script recursion faults
  trappably instead of overflowing the native stack.

Scoped use only in this release: call the closure while your host
function runs. A stored-callback API (`Vm::call_closure` on a kept
value) is a planned follow-up. Callbacks are for free functions;
opaque-type *methods* cannot take `HostCtx` yet (the receiver borrow
would be held across re-entry).

## Shared live values

`Shared<T>` is a host-side handle to a live script value — both sides
observe mutation:

```rust
let raw = vm.call_values(&unit, "make_state", vec![])?;
let state: Shared<GameState> = Shared::from_value(raw, &unit);

state.get()?;                  // convert a snapshot out (data types)
state.set(new_state)?;         // write fields through the live value

let pane: Shared<Pane> = ...;  // opaque types: direct borrows
pane.borrow()?.title;          
pane.borrow_mut()?.title = "renamed".into();
```

Aliasing violations at the host boundary — the script calling a `&mut`
method while you hold a `borrow()`, double borrows through `Shared` —
surface as **`Err`, never panics**.

## Faults

`vm.call*` returns `Err(Error::Runtime(_))` for trappable VM faults
(out-of-bounds, division by zero, stub interface calls, aliasing
violations…), with a message, source span and script stack trace. Script
code never observes a Rust panic.

## Resource limits

Three limits are enforced, one is deliberately not:

- **Call depth.** Script recursion faults ("stack overflow") past a
  per-VM limit — default 10,000 frames, tune it with
  `vm.set_call_depth_limit(n)`. Frames live on a heap-allocated register
  stack, so the limit counts calls, not thread-stack bytes.
- **Fuel.** `vm.set_fuel(Some(n))` gives the VM an execution budget:
  every dispatched instruction costs 1 fuel (a host call costs 1 for the
  dispatch; what the host function does internally is not metered), and
  running dry faults ("fuel exhausted") with the usual span and stack
  trace. Fuel counts instructions, not time, so exhaustion is
  deterministic across machines. Accounting is exact but charged at
  control-transfer points (jumps, calls, returns), so the fault lands at
  the end of the current straight-line run of instructions — never past
  a host call or a loop iteration. The tank belongs to the `Vm` and
  depletes across calls until you set it again — set it before each call
  for a per-call budget, or once per tick for a shared one. `vm.fuel()`
  reads the remainder (what a call cost is `before - after`);
  `set_fuel(None)` (the default) runs unmetered. A possible follow-up is
  letting host functions charge fuel for expensive operations via
  `HostCtx`; today host-op cost is host-side only.
- **Nesting.** The compiler rejects pathologically nested/chained source
  (E0114/E0271) long before it could exhaust the compile stack.
- **Memory is not limited.** A script can allocate until the allocator
  refuses (`let mut l = []` + a hot `push` loop will get there). If you
  run untrusted scripts and OOM matters, sandbox at the process level
  (cgroups, job objects, a worker process) — the VM does not meter
  allocations. (Fuel indirectly caps the allocation *rate* per budget,
  which is often the practical concern.)

## Threading

```rust
let ctx = Context::new().module(...);      // Send + Sync
let unit = ctx.compile(src)?;              // Send + Sync
std::thread::spawn({
    let (ctx, unit) = (ctx.clone(), unit.clone());
    move || {
        let mut vm = Vm::new(&ctx);        // one VM per thread
        let _: i64 = vm.call_unit(&unit, "main", ()).unwrap();
    }
});
```

Script *values* never cross threads (`Rc`, not `Arc` — PRD §4.3). Your
registered closures must be `Send + Sync` (they are shared), but they only
ever run on the calling VM's thread.

## Interface files for the LSP

The editor has never seen your host API. Fix that with one line:

```rust
ctx.write_interface("api.wscripti")?;
```

and a `wscript.toml` next to your scripts:

```toml
interfaces = ["api.wscripti"]
```

`wscript check` and `wscript lsp` read the manifest and typecheck scripts
against the declared API — completions, hover, goto-definition included.
`.wscripti` files are a strict subset of wscript syntax (declarations only,
think `.d.ts`) and are diff-friendly; regenerate them in CI to keep them
honest. The stdlib ships its own (`wscript-std/wscripti/std.wscripti`).

## Feature gates

The umbrella crate re-exports the stdlib behind the default `std`
feature; each stdlib module is also its own Cargo feature in `wscript-std`
(`math`, `fs`, `process`, `json`, `toml`, `xml` — default all). Registering
a module is what grants the capability: don't register `fs`, and scripts
cannot touch the filesystem.
