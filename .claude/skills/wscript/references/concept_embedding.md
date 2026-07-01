# Embedding in Rust

_Context + Module + Vm: register host APIs, compile once (Send+Sync), run one Vm per thread._

wscript's reason to exist: exposing host functions and types is one derive and one
registration call, with plain Rust signatures, and **all** type errors — including
misuse of your host API — surface when the script compiles, not when it runs.


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

## The pieces

| Piece | Role |
| --- | --- |
| `Context` | host registrations + compiler; `Send + Sync`, cheaply cloneable |
| `Module` | a named bag of functions, constants and types; scripts opt in with `use name` |
| `Vm` | per-thread execution; `!Send` (script values are `Rc`-managed) |
| `CompiledUnit` | the output of `ctx.compile(source)`; `Send + Sync` |

Nothing is ambient (capability-style): a script can only touch what you registered. Compile once, share the `Context` and `CompiledUnit` across threads, and spin one `Vm` per thread. See [interop types](../references/concept_interop_types.md) for exposing data, and the **Embed wscript in a Rust application** process for the full runbook.

## Calling script from Rust

```rust
// One-shot, types checked at the boundary:
let n: i64 = vm.call(&unit, "compute", (5i64, "scale"))?;

// Hot path — typed handle, signature verified once at lookup:
use wscript::UnitExt;
let on_key: wscript::ScriptFn<(KeyEvent,), bool> = unit.fn_handle("on_key")?;
let quit = on_key.call(&mut vm, (key_event,))?;   // cheap thereafter
```

## Examples

### Embedding — register a module and call a script function

Register a host `Module`, fold in a stdlib module, compile (all type errors surface here), then run `main` on a per-thread Vm.

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
    fn main() -> int {
        term::print_at(1, 2, "hello")
        term::MAX_PANES
    }
"#)?;                                   // ← ALL type errors surface here

let mut vm = Vm::new(&ctx);
let n: i64 = vm.call_unit(&unit, "main", ())?;
```

**Expected:** n == 16. Calling `term::print_at("oops", 1, 2)` would instead be a compile error.

## Related

- [Interop: Data vs Opaque Types](../references/concept_interop_types.md)

- [Interface Files & wscript.toml](../references/concept_interface_files.md)

- [Modules & use](../references/concept_modules.md)

- [The Type System](../references/concept_type_system.md)

[← Back to SKILL.md](../SKILL.md)
