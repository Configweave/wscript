# Embedding wisp in a Rust application

wisp's reason to exist is PRD §6: exposing host functions and types is one
derive and one registration call, with plain Rust signatures, and **all**
type errors — including misuse of your host API — surface when the script
compiles, not when it runs.

```toml
[dependencies]
wisp = { path = "..." }          # umbrella crate: everything you need
```

## The 60-second version

```rust
use wisp::{Context, Module, Vm};

let mut m = Module::new("term");
m.fn_("clear", || { /* ... */ });
m.fn_("print_at", |x: i64, y: i64, s: &str| { /* ... */ });
m.const_("MAX_PANES", 16i64);

let ctx = Context::new()
    .module(wisp::std_modules::math())
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
`#[script(crate_path = "wisp_core")]` for crates that depend on
`wisp-core` directly.

## Calling script from Rust

```rust
// One-shot, types checked at the boundary:
let n: i64 = vm.call(&unit, "compute", (5i64, "scale"))?;

// Hot path — typed handle, signature verified once at lookup:
use wisp::UnitExt;
let on_key: wisp::ScriptFn<(KeyEvent,), bool> = unit.fn_handle("on_key")?;
let quit = on_key.call(&mut vm, (key_event,))?;   // cheap thereafter
```

`fn_handle` fails with `Error::Signature` if the script function's
signature doesn't match `(A...) -> R`.

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
ctx.write_interface("api.wispi")?;
```

and a `wisp.toml` next to your scripts:

```toml
interfaces = ["api.wispi"]
```

`wisp check` and `wisp lsp` read the manifest and typecheck scripts
against the declared API — completions, hover, goto-definition included.
`.wispi` files are a strict subset of wisp syntax (declarations only,
think `.d.ts`) and are diff-friendly; regenerate them in CI to keep them
honest. The stdlib ships its own (`wisp-std/wispi/std.wispi`).

## Feature gates

The umbrella crate re-exports the stdlib behind the default `std`
feature; each stdlib module is also its own Cargo feature in `wisp-std`
(`math`, `fs`, `process`, `json`, `toml`, `xml` — default all). Registering
a module is what grants the capability: don't register `fs`, and scripts
cannot touch the filesystem.
