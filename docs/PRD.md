# PRD: wisp — an embeddable scripting language for Rust

**Status:** Draft v1 for implementation
**Working name:** `wisp` (placeholder — rename is a find/replace; note "Wren" is taken by an existing embeddable language)
**Target:** Rust workspace, implemented incrementally per the milestones in §10

---

## 1. Summary

wisp is a statically typed, Rust-flavored scripting language designed to be embedded in Rust applications, occupying the niche Lua occupies for C programs. It compiles to bytecode and runs on a register-based VM. Its differentiating features over Lua/Rhai/Rune:

1. **Static typing with inference** — type errors, including misuse of *host-registered* APIs, are caught at script compile time, not at runtime.
2. **First-class Rust interop** — exposing host functions and types is one derive and one registration call, with plain Rust signatures and no manual value wrangling.
3. **Rust-stripped syntax** — structs, enums, traits, exhaustive `match`, `Option`/`Result` with `?`. Familiar to Rust developers, learnable by non-Rust developers in an afternoon.
4. **No borrow checker** — Rc-based memory model with free aliased mutation, Lua/Python mental model.

### Primary use case

Scripting layer for the author's Rust tools (terminal multiplexer, editor, task runner). General-purpose embeddability is the design target; those tools are the first consumers.

### Non-goals (v1)

- Async/await (sync only; async is a v2 consideration — do not paint the VM design into a corner, but do not build it)
- Thread-safe script values (`Rc`, not `Arc`; one VM per thread)
- User-defined generics (see §3.6)
- Script types implementing host traits (v2, see §6.6)
- A tracing GC or cycle collector (see §4.2)
- JIT compilation
- Package manager / module registry

---

## 2. Architecture overview

Cargo workspace:

```
wisp/            # umbrella crate: embedding API, re-exports compiler+vm+std
wisp-compiler/   # lexer, parser, AST, type checker, bytecode emitter
wisp-vm/         # register-based VM, value model, Rc runtime
wisp-std/        # stdlib modules, individually feature-gated
wisp-cli/        # binary: run, check, repl, lsp subcommands
```

Dependency direction: `wisp-cli → wisp → {wisp-compiler, wisp-vm, wisp-std}`. The compiler and VM share a `wisp-core`-style set of common types (bytecode format, type representations, constants) — either a small shared crate or a module re-exported from `wisp-compiler`; implementer's choice, but the VM must not depend on the parser.

Embedding flow:

```rust
let ctx = Context::new()
    .module(wisp_std::math())
    .module(my_app_module());     // host API
let unit = ctx.compile(source)?;  // ALL type errors surface here
let mut vm = Vm::new(&ctx);
let result: i64 = vm.call_unit(&unit, "main", ())?;
```

Key invariant: **the type checker knows every host signature before checking a script.** Registration happens up front in a `Context`; scripts compile against that context.

---

## 3. Language specification

### 3.1 Lexical structure & syntax flavor

- Rust-style tokens: `//` and `/* */` comments, snake_case convention, `::` for module paths.
- **Expression-oriented**: blocks evaluate to their last expression; `if`/`match` are expressions.
- **Newline-terminated statements; semicolons optional** (permitted, never required). A statement continues across newlines when the line obviously cannot end (trailing binary operator, open delimiter). Keep the rule simple and document it precisely in the grammar.
- String literals: `"..."` with standard escapes; string interpolation is **deferred to v2** (use `string::format` / a `fmt` builtin in v1).

### 3.2 Types

Primitives (value types, stored inline in registers):

| wisp type | representation |
|---|---|
| `int` | `i64` |
| `float` | `f64` |
| `bool` | `bool` |
| `char` | `char` |
| `unit` | `()` — the empty block/return type |

Reference types (Rc-managed, see §4):

- `string` (immutable value semantics at the language level; Rc internally for cheap clones)
- `struct` and `enum` instances
- `List[T]`, `Map[K, V]` — built-in containers (§3.6)
- function values / closures
- `weak[T]` (§4.2)

### 3.3 Declarations

```rust
let x = 5                 // inferred: int
let name: string = "wil"  // annotation allowed anywhere, required nowhere on lets

fn area(w: int, h: int) -> int {   // annotations REQUIRED on fn params & return
    w * h                          // (return type omitted = unit)
}

struct Point { x: int, y: int }

enum Event {
    Quit,                          // unit variant
    Key(char),                     // tuple variant
    Click { x: int, y: int },      // struct variant
}

trait Damageable {
    fn take_damage(self, amount: int)
}

impl Point {
    fn manhattan(self) -> int { math::abs(self.x) + math::abs(self.y) }
}

impl Damageable for Point {
    fn take_damage(self, amount: int) { ... }
}
```

**Typing discipline:** annotations are mandatory on function signatures (params and non-unit returns) and struct/enum fields; inferred everywhere else (locals, closure params where the context determines them). This keeps inference local and simple — no Hindley–Milner, no inference across function boundaries.

**`self`** is implicit in type and is always the receiver by reference (everything is by-reference anyway, §4.1). No `&self`/`&mut self` distinction — `&` does not exist in the surface language.

### 3.4 Control flow & pattern matching

- `if`/`else if`/`else` (expression), `while`, `loop`, `for x in iterable`, `break`/`continue`, `return`.
- `match` is an expression and **exhaustiveness-checked at compile time** — missing enum variants are compile errors. Supported patterns in v1:
  - literals, `_` wildcard, identifier bindings
  - enum destructuring (all three variant kinds), struct destructuring
  - or-patterns (`A | B`), guards (`pat if cond`)
- `if let pat = expr { ... } else { ... }` and `let pat = expr else { ... }` (else block must diverge).
- **Deferred past v1:** range patterns, `@` bindings. Nested patterns should work, but exhaustiveness analysis is only *guaranteed* to one level of nesting in v1 — deeper nesting may conservatively require `_`. Flag this in diagnostics rather than silently miscounting. This is a known risk area; budget time for it.

### 3.5 Error handling

- `Option[T]` and `Result[T, E]` are stdlib enums (always available, no import).
- `?` operator on both, with the same semantics as Rust (early-return the `None`/`Err`).
- Host function errors map to script-level `Result` (§6.3) so `?` composes across the host boundary.
- No exceptions. VM-level faults (e.g., index out of bounds, RefCell-style aliasing at the host boundary) surface as `Result` errors or, for true bugs, a trappable VM fault the *host* receives as `Err` — script code never catches panics. A fault carries the message, the faulting span, and a full script-level stack trace (one frame per call, innermost first, each with a source span); the CLI renders each frame as `file:line:col`.

### 3.6 Generics: the deliberate cut

- **No user-defined generics in v1.** All user functions are monomorphic.
- `List[T]`, `Map[K, V]`, `Option[T]`, `Result[T, E]`, `weak[T]` are **compiler-special-cased built-ins**. The checker knows their methods' type schemes internally; users instantiate them but cannot define their own generic types or functions.
- Square-bracket type application (`List[int]`) is reserved syntax so v2 user generics don't need a syntax break.
- Closures may be polymorphic *in inference* only where fully determined by context; they are monomorphic once checked.

### 3.7 Traits

- Go-interface-flavored: a `trait` declares method signatures; `impl Trait for Type` provides them.
- **Static dispatch** when the concrete type is known (the common case — direct call, no vtable).
- **Dynamic dispatch** via `dyn Trait` as a type (`fn draw(w: dyn Widget)`), implemented with a vtable attached to the value. Coercion from concrete type to `dyn Trait` is implicit at typed boundaries.
- Operator overloading via built-in traits: `Add`, `Sub`, `Mul`, `Div`, `Rem`, `Neg`, `Eq`, `Ord`, `Display`, `Index`. `==` on structs/enums **requires** an `Eq` impl (derivable, §3.8) — there is no implicit identity equality. Reference identity is available explicitly as a builtin (`same(a, b)`).
- No default method bodies in v1 (v2). No trait inheritance in v1.

### 3.8 Derives

`#[derive(...)]` on structs/enums for: `Eq`, `Ord`, `Display` (debug-ish formatting), `Clone` (deep clone — note plain assignment never clones, §4.1).

### 3.9 Modules & imports

- One file = one script compilation unit in v1. `use module_name` / `use module_name::item` imports from *registered* modules (host or stdlib).
- **Script-to-script imports are v2.** v1 scripts are single-file. (The CLI may later grow multi-file support; do not block on it.)

---

## 4. Memory & mutation model

### 4.1 Reference semantics

- Primitives are value types. **Everything else is an Rc-managed reference type.**
- Assignment, argument passing, and returns of reference types copy the *reference* (Rc clone), never the data. Two variables can refer to the same struct; mutation through either is visible through both. This is the Lua/Python mental model, stated explicitly in the docs.
- **Free mutation, no borrow checker, no `&`/`&mut` anywhere in the surface language.** Internally the VM may use `Rc<RefCell<...>>` or unsafe-cell equivalents; aliasing rules are the implementer's problem, not the script author's. The single-threaded VM (§4.3) makes this sound.
- `string` is immutable (operations return new strings), so it behaves like a value type despite Rc internals.

### 4.2 Cycles & weak references

- **Pure reference counting. Cycles leak. This is accepted and documented.**
- `weak[T]` in the language (create via `weak(x)`, read via `.upgrade() -> Option[T]`) for back-references in cyclic structures.
- No cycle collector in v1 or planned. A debug-mode leak/cycle detector is a *possible* future CLI tool, not a runtime feature.

### 4.3 Threading

- `Rc`, not `Arc`. One VM per thread; script values are not `Send`. The `Vm` and `Context` types should be `!Send` or document the constraint via the type system.
- `Context` (compiled signatures, no live values) **should** be shareable/cloneable across threads so multiple per-thread VMs can be spun from one compilation — verify this falls out of the design; it's valuable for the multiplexer use case.

---

## 5. Compiler & VM

### 5.1 Compiler pipeline

`source → lexer → parser (AST) → resolver (names/modules) → type checker → bytecode emitter → CompiledUnit`

- Hand-written recursive-descent parser (error recovery matters for the LSP — the parser must produce a partial AST + diagnostics on broken input, not bail on first error).
- Type checker: bidirectional checking with local inference per §3.3. Produces fully typed AST. All diagnostics carry spans.
- Diagnostics: structured (code, span, message, optional help), rendered nicely in the CLI (ariadne or codespan-reporting style) and consumed raw by the LSP.

### 5.2 VM

- **Register-based bytecode** (Lua 5.x style): fixed-width instructions, per-function register file, constants table per unit.
- Call convention supports host calls and script calls uniformly; `ScriptFn` handles (§6.4) resolve to a function index once.
- Values: a `Value` enum with inline primitives and Rc payloads for reference types. NaN-boxing is an *optional* optimization — do not start there; start with a plain enum and benchmark later.
- Bytecode format is **internal and unstable** in v1 (no serialization guarantee). Design the instruction set in its own module with docs, since it will be revisited.

---

## 6. Rust interop (the product)

This is the most important section. The embedding API must feel like the examples below — if implementation pressure pushes the API away from this shape, that's a design conversation, not a silent compromise.

### 6.1 Registering functions

```rust
let mut m = Module::new("term");
m.fn_("clear", || term::clear());
m.fn_("print_at", |x: i64, y: i64, s: &str| term::print_at(x, y, s));
m.fn_("read_key", || -> Result<char, TermError> { ... });  // Err → script Result
m.const_("MAX_PANES", 16i64);
```

- Implemented via a sealed trait over `Fn` arities (macro-generated for 0..=N args).
- Registration captures the **type signature** for the checker — calling `print_at("oops", 1, 2)` from a script is a compile-time error with a proper diagnostic.
- Supported parameter/return types: primitives, `String`/`&str`, `Vec<T>`, `HashMap<K, V>`, `Option<T>`, `Result<T, E>`, and any `#[derive(Script)]` type — i.e., anything `FromValue + IntoValue` (§6.3).

### 6.2 Exposing types

```rust
#[derive(Script)]                 // DATA type: fields visible in script,
struct Point { x: i64, y: i64 }   // constructible (Point { x: 1, y: 2 }) and
                                  // destructurable in match

#[derive(Script)]
#[script(opaque)]                 // HANDLE type: opaque resource,
struct Pane { /* internals */ }   // no field access, methods only

m.ty::<Pane>()
 .method("title", |p: &Pane| p.title.clone())
 .method("split", |p: &mut Pane, d: Dir| ...);
```

- Data types convert **by value** across the boundary (script constructs a `Point`, Rust receives a real `Point`).
- Opaque types cross **by handle** (script holds a reference to the live Rust value).
- Enums derive too — a Rust enum with `#[derive(Script)]` is matchable in script, including payloads, subject to the same field-type rules.

### 6.3 Value conversion

- `FromValue` / `IntoValue` traits, implemented for primitives, `String`, `Vec`, `HashMap`, `Option`, `Result`, tuples (for multi-arg calls), and derived types.
- Script `Result`/`Option` ↔ Rust `Result`/`Option` map structurally, so `?` composes across the boundary in both directions.

### 6.4 Calling script from Rust

```rust
// One-shot, types checked at the boundary:
let n: i64 = vm.call("compute", (5, "scale"))?;

// Hot path — typed handle, signature verified once at lookup:
let on_key: ScriptFn<(char,), bool> = unit.fn_handle("on_key")?;
let quit = on_key.call(&mut vm, ('q',))?;   // cheap thereafter
```

### 6.5 Shared live values

- `Shared<T>`: host-side handle to a live script value (or a host value inserted into the VM). Both sides observe mutation.
- Aliasing violations at the host boundary (host borrows while script mutates, etc.) surface as **`Err`, never panics.**

### 6.6 Deferred to v2

Script types implementing *host* traits (Rust code taking `Box<dyn Widget>` backed by a script object). Use `ScriptFn` callbacks for plugin patterns in v1.

---

## 7. Standard library (`wisp-std`)

Every module is opt-in at embed time (capability-style — nothing ambient) and feature-gated in Cargo. The CLI enables all of them by default.

| module | contents | notes |
|---|---|---|
| `math` | abs, min, max, clamp, floor/ceil/round/trunc, sqrt, pow, exp, ln/log2/log10, full trig + hyperbolic, lerp, signum, consts (PI, E, TAU, INF, NAN), `rand()`, `rand_range(a, b)` | pure; always safe |
| `string` | len, split, join, trim variants, to_upper/lower, starts/ends_with, contains, find, replace, repeat, pad_left/right, chars, slice, parse_int/float, fmt | `string` methods may also hang off the type via built-in impls — pick one surface and be consistent |
| `fs` | read_to_string, read_bytes, write, append, exists, is_file/is_dir, list_dir, create_dir(_all), copy, rename, remove_file/dir, path helpers (join, parent, file_name, ext) | capability: file I/O |
| `process` | `run(cmd, args) -> Result[Output, E]` where `Output { status: int, stdout: string, stderr: string }`; `env(key)`, `set_env`, `cwd`, `args()` | capability: command execution |
| `json` | `parse(string) -> Result[Value, E]`, `to_string(Value)`, `to_string_pretty` | serde_json |
| `toml` | same surface | toml crate |
| `xml` | same surface, **with documented mapping convention** | quick-xml; see wart below |

**Shared dynamic `Value` type:** json/toml/xml all parse into one stdlib `Value` enum (`Null`, `Bool`, `Int`, `Float`, `String`, `List`, `Map`) so formats compose (parse JSON, emit TOML). It is the *one* dynamically-typed escape hatch in the language; accessors return `Option`/`Result` (`v.get("key")`, `v.as_int()`).

**Known wart (decided, not open):** XML doesn't fit a JSON-shaped value model. Convention: attributes under an `"@attrs"` map key, text content under `"#text"`, repeated child elements collapse to lists. Document it prominently; do not invent a second value type for XML.

---

## 8. CLI (`wisp-cli`)

Binary name `wisp`:

- `wisp run <file> [args...]` — compile + execute, full stdlib, script args via `process::args()`. Exit code from script `main`'s return (int or unit→0) or 1 on error.
- `wisp check <file>` — compile only, print diagnostics, exit code reflects success. Honors `.wispi` files (§9).
- `wisp repl` — line-oriented REPL. Best-effort: persistent top-level `let` bindings across lines; acceptable to re-compile incrementally. Keep it simple.
- `wisp lsp` — start the language server on stdio.
- `wisp fmt` — **stretch goal**, only if trivial; do not block v1 on a formatter.

Pretty diagnostics with source spans, colors when TTY. Runtime faults
render the message, a source snippet at the fault site, and a full script
stack trace with per-frame `file:line:col`.

## 9. LSP

- Built on `tower-lsp`, launched via `wisp lsp` (stdio).
- **v1 features, in priority order:**
  1. Diagnostics (straight reuse of compiler errors; requires the error-recovering parser from §5.1)
  2. Hover (inferred types, signatures, doc strings on host decls if present)
  3. Go-to-definition (script-local symbols; host symbols jump to `.wispi` entry)
  4. Completions (keywords, in-scope symbols, module members, struct fields, enum variants after `::`, methods after `.`)
- **Deferred:** rename, find-references, semantic tokens, formatting, code actions.

### 9.1 Declaration files (`.wispi`) — the host-API problem

The point of wisp is scripting against host-registered APIs the LSP has never seen. Fix:

- `Context::write_interface(path)` dumps every registered module/function/type/const signature to a `.wispi` interface file — a *textual, human-readable* subset of wisp syntax (declarations only, no bodies), versioned and diff-friendly. Think `.d.ts`.
- A small project manifest (`wisp.toml`) in the workspace root lists `.wispi` files (and later, settings). The LSP and `wisp check` both honor it.
- `wisp-std` ships generated `.wispi` files for its modules so the LSP works out of the box for CLI scripts.
- The `.wispi` grammar is a strict subset of the language grammar — parse it with the same parser.

---

## 10. Milestones

Each milestone should land with tests passing and an updated `examples/` script demonstrating the new surface.

1. **M1 — Core pipeline:** lexer, parser (with recovery), AST; `let`, functions, primitives, arithmetic/comparison, `if`/`while`/blocks; type checker for the above; bytecode + VM executing it; `wisp run`/`wisp check` minimal. *Gate: fib/fizzbuzz scripts run.*
2. **M2 — Data types:** structs, impl blocks, methods; enums, `match` with exhaustiveness, `if let`/`let else`; `Option`/`Result` + `?`; `List`/`Map` built-ins with literals (`[1,2,3]`, map literal syntax of implementer's choosing — propose, don't agonize); `for` loops; `string` type + methods. *Gate: a non-trivial script (e.g., a small interpreter or task-list app) runs.*
3. **M3 — Traits & Rc semantics:** traits, impl Trait for Type, `dyn Trait` vtable dispatch, operator traits, derives; `weak[T]`; reference-semantics test suite (aliasing, mutation visibility, cycle-leak documented behavior).
4. **M4 — Interop:** `Context`/`Module`/registration, `#[derive(Script)]` (data + opaque), `FromValue`/`IntoValue`, `vm.call`, `ScriptFn`, `Shared<T>`, host-signature type checking with good diagnostics. *Gate: an example host app (mini TUI or echo server) scripted end to end.*
5. **M5 — Stdlib:** all §7 modules + the shared `Value` type; feature gates; `.wispi` generation for std.
6. **M6 — Tooling:** REPL; LSP with the four v1 features; `wisp.toml` + `.wispi` consumption; editor smoke test (VS Code via generic LSP client is sufficient — no extension required for v1).
7. **M7 — Polish:** diagnostics pass (every error has a help text), docs (language tour + embedding guide + stdlib reference), benchmark vs Lua/Rhai/Rune on a few microbenchmarks (honest numbers, whatever they are).

## 11. Testing requirements

- Compiler: snapshot tests for diagnostics (insta or similar); golden tests parser → AST.
- Type checker: positive/negative test pairs per feature, especially match exhaustiveness and host-signature misuse.
- VM: behavior tests written *in wisp* under `tests/scripts/`, runner asserts on output/exit.
- Interop: round-trip conversion tests for every `FromValue`/`IntoValue` impl; aliasing-violation-returns-Err tests.
- Fuzzing the parser (cargo-fuzz) is a stretch goal.

## 12. Risks & known tradeoffs (decided — do not relitigate without flagging)

| risk | stance |
|---|---|
| RC cycles leak | Accepted. `weak[T]` provided. Document loudly. |
| Exhaustiveness on deeply nested patterns | Guaranteed to one nesting level in v1; conservative beyond. Budget extra time here. |
| Monomorphic-only + built-in generic containers | Accepted v1 cut; `[T]` syntax reserved for v2 generics. |
| XML ↔ Value mapping is lossy/ugly | Accepted with the §7 convention. |
| LSP scope creep | Four features. The list in §9 is a ceiling. |
| Static checker vs "minimal syntax" tension | Resolved: annotations on signatures only, local inference elsewhere. |

## 13. Open items (implementer proposes, author approves)

- Map literal syntax (M2)
- `fmt`/string-formatting builtin surface (until v2 interpolation)
- Exact statement-termination/continuation rule wording (§3.1)
- Whether `Context` cross-thread sharing falls out cleanly (§4.3)

---

## Appendix A — Script sample (target surface)

```rust
use math
use fs

struct Player {
    name: string,
    hp: int,
}

enum Event {
    Quit,
    Key(char),
    Click { x: int, y: int },
}

trait Damageable {
    fn take_damage(self, amount: int)
}

impl Damageable for Player {
    fn take_damage(self, amount: int) {
        self.hp = math::max(0, self.hp - amount)
    }
}

fn handle(e: Event, p: Player) -> bool {
    match e {
        Event::Quit => false,
        Event::Key(c) => {
            if c == 'x' { p.take_damage(10) }
            true
        }
        Event::Click { x, y } => {
            // structs are reference types: mutation here is visible to callers
            p.hp = p.hp + 1
            x >= 0 && y >= 0
        }
    }
}

fn main() -> int {
    let p = Player { name: "wil", hp: 100 }
    let alias = p              // same object, not a copy
    alias.take_damage(30)
    let log = "hp: " + str(p.hp)   // p.hp == 70
    fs::write("game.log", log)?
    0
}
```

## Appendix B — Embedding sample (target surface)

```rust
use wisp::{Context, Module, Vm, ScriptFn};

#[derive(Script)]
struct KeyEvent { code: char, ctrl: bool }

#[derive(Script)]
#[script(opaque)]
struct Pane { /* ... */ }

fn pane_module() -> Module {
    let mut m = Module::new("panes");
    m.ty::<Pane>()
        .method("title", |p: &Pane| p.title.clone())
        .method("split", |p: &mut Pane, vertical: bool| p.split(vertical));
    m.fn_("active", || current_pane());
    m
}

fn main() -> anyhow::Result<()> {
    let ctx = Context::new()
        .module(wisp_std::math())
        .module(wisp_std::string())
        .module(pane_module());

    ctx.write_interface("api.wispi")?;          // for the LSP

    let unit = ctx.compile(&std::fs::read_to_string("init.wisp")?)?;
    let mut vm = Vm::new(&ctx);

    let on_key: ScriptFn<(KeyEvent,), bool> = unit.fn_handle("on_key")?;
    let handled = on_key.call(&mut vm, (KeyEvent { code: 'q', ctrl: true },))?;
    Ok(())
}
```
