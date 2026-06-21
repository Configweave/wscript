# wscript

An embeddable, statically typed, Rust-flavored scripting language —
occupying the niche for Rust applications that Lua occupies for C
programs. Compiles to bytecode, runs on a register-based VM.

```rust
use math
use fs

struct Player { name: string, hp: int }

trait Damageable { fn take_damage(self, amount: int) }

impl Damageable for Player {
    fn take_damage(self, amount: int) {
        self.hp = math::imax(0, self.hp - amount)
    }
}

fn main() -> int {
    let p = Player { name: "wil", hp: 100 }
    let alias = p                  // reference semantics: same object
    alias.take_damage(30)
    fs::write("game.log", fmt("hp: {}", p.hp))?   // p.hp == 70
    0
}
```

What sets it apart from Lua/Rhai/Rune:

1. **Static typing with inference** — type errors, *including misuse of
   host-registered APIs*, are caught when the script compiles, not when
   it runs. Annotations on function signatures only; local inference
   everywhere else. Exhaustive `match`, `Option`/`Result` with `?`.
2. **First-class Rust interop** — exposing host functions and types is
   one derive and one registration call with plain Rust signatures:

   ```rust
   #[derive(Script)]
   struct Point { x: i64, y: i64 }

   let mut m = Module::new("term");
   m.fn_("print_at", |x: i64, y: i64, s: &str| { /* ... */ });
   let ctx = Context::new().module(m);
   let unit = ctx.compile(source)?;   // ALL type errors surface here
   ```

3. **Rust-stripped syntax** — structs, enums, traits (incl. `dyn`
   dispatch), derives — familiar to Rust developers, learnable by anyone
   in an afternoon.
4. **No borrow checker** — Rc-based memory with free aliased mutation;
   the Lua/Python mental model. Cycles leak (documented); `weak[T]`
   provided.

## Crates

| crate | contents |
|---|---|
| `wscript` | umbrella: embedding API (`Context`, `Vm`, `ScriptFn`, `Shared`), re-exports |
| `wscript-core` | shared types: spans, diagnostics, type repr, bytecode, values, registry |
| `wscript-compiler` | lexer, error-recovering parser, type checker, bytecode emitter |
| `wscript-vm` | register-based VM (one per thread; `Context`/units shared across threads) |
| `wscript-std` | stdlib: math, fs, process, json/toml/xml over one `Value` type — capability-style, feature-gated |
| `wscript-macros` | `#[derive(Script)]` |
| `wscript-cli` | the `wscript` binary: `run`, `check`, `repl`, `lsp` |

## CLI

```sh
wscript run script.wscript [args...]   # compile + execute (full stdlib)
wscript check script.wscript           # compile only; honors wscript.toml/.wscripti
wscript repl                        # persistent-binding REPL
wscript lsp                         # language server (diagnostics, hover,
                                 # goto-definition, completions)
```

## Documentation

- [Language tour](docs/tour.md)
- [Embedding guide](docs/embedding.md)
- [Stdlib reference](docs/stdlib.md)
- [Benchmarks](docs/benchmarks.md) — beats Rhai, trades blows with Rune, trails Lua
- [PRD](docs/PRD.md) — the design source of truth

## Building

Standard Rust toolchain: `cargo build`, `cargo test`, `cargo clippy`.
Examples live in `examples/` (run with `wscript run`) and
`wscript/examples/host_app.rs` (`cargo run -p wscript --example host_app`).
