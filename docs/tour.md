# The wscript language tour

wscript is a statically typed, Rust-flavored scripting language that embeds in
Rust applications the way Lua embeds in C programs. If you know Rust, you
already know most of wscript — subtract the borrow checker, lifetimes, and
generics, and you are there. If you don't know Rust, this tour is still an
afternoon read.

Run any example with `wscript run file.wscript`. Execution starts at `fn main()`;
its `int` return becomes the exit code (`unit` means 0).

## Values and types

Primitives (value types):

| type | example |
|---|---|
| `int` | `42`, `-7`, `0xFF`, `1_000_000` (64-bit signed, wrapping) |
| `float` | `3.14`, `1e9` (64-bit IEEE) |
| `bool` | `true`, `false` |
| `char` | `'a'`, `'\n'`, `'\u{1F600}'` |
| `unit` | `()` — the empty type |

Everything else is a **reference type**: `string`, structs, enums,
`List[T]`, `Map[K, V]`, function values, and `weak[T]`.

```rust
let x = 5                  // inferred: int
let name: string = "wil"   // annotations allowed anywhere on lets,
let pi = 3.14              // required nowhere

let log = "hp: " + str(99) // + concatenates strings; str() converts
let msg = fmt("{} of {}", 3, 10)   // {} placeholder formatting
```

There is **no implicit numeric conversion**: `1 + 2.0` is a type error.
Convert explicitly with `int(x)` (truncates) and `float(x)`.

There is **no truthiness**: conditions must be `bool`.

## Statements end at newlines

Semicolons are permitted, never required. A statement continues across a
newline only when the line obviously cannot end:

```rust
let total = add(1,
    2, 3)              // open ( — newlines don't terminate inside ( or [

let s = "hello"
    .to_upper()        // a line starting with `.` continues the chain

if total > 5 {
    println("big")
}
else {                 // `else` may start a line
    println("small")
}
```

The precise rule: a newline ends the statement unless (a) it is inside an
unclosed `(` or `[`, (b) the parser just consumed a token that cannot end
an expression (a binary operator, `,`, `=`, …), (c) the next line starts
with `.`, or (d) the next token is `else`.

A trailing `;` discards a block's tail value (Rust-style): a function
declared without a return type must not end with a non-unit expression —
write `f();` to discard.

## Functions

Annotations are **required on function signatures** and nowhere else
(this is what keeps inference local and compile errors readable):

```rust
fn area(w: int, h: int) -> int {
    w * h                  // blocks evaluate to their last expression
}

fn log(msg: string) {      // omitted return type = unit
    println(msg)
}
```

Function values and closures:

```rust
fn apply(f: fn(int) -> int, x: int) -> int { f(x) }

fn make_counter() -> fn() -> int {
    let n = 0
    || { n = n + 1; n }    // closures capture by reference
}

let double = |x| x * 2     // params inferred where context determines them
apply(double, 21)          // 42
```

## Structs, enums, methods

```rust
struct Player { name: string, hp: int }

enum Event {
    Quit,                        // unit variant
    Key(char),                   // tuple variant
    Click { x: int, y: int },    // struct variant
}

impl Player {
    fn heal(self, amount: int) {
        self.hp = self.hp + amount
    }
}
```

`self` is implicit in type and always by reference — there is no `&` in
wscript at all.

**Reference semantics** (the Lua/Python mental model, stated explicitly):
assignment, argument passing and returns of reference types copy the
*reference*, never the data.

```rust
let p = Player { name: "wil", hp: 100 }
let alias = p              // same object
alias.hp = 70
p.hp                       // 70 — mutation visible through both
same(p, alias)             // true — reference identity builtin
```

Plain assignment never clones. Deep cloning is explicit:

```rust
#[derive(Clone)]
struct Config { values: List[int] }
let copy = config.clone()  // deep copy
```

## Pattern matching

`match` is an expression and is **exhaustiveness-checked at compile
time** — a missing variant is a compile error with a witness:

```rust
fn handle(e: Event) -> bool {
    match e {
        Event::Quit => false,
        Event::Key(c) if c == 'q' => false,   // guards
        Event::Key('h') | Event::Key('?') => help(),  // or-patterns, literals
        Event::Key(_) => true,
        Event::Click { x, y } => x >= 0 && y >= 0,    // struct destructuring
    }
}
```

Arms with `if` guards never count toward exhaustiveness. Bindings inside
or-patterns are not supported in v1. Exhaustiveness analysis is exact for
nested variant/struct/bool/literal patterns.

`if let` and `let ... else` work as in Rust (`let-else`'s block must
diverge):

```rust
if let Some(task) = store.find(id) { task.done = true }

let Some(n) = s.parse_int() else { return Err("not a number") }
```

## Option, Result and `?`

`Option[T]` and `Result[T, E]` are always available, no import. `?`
early-returns the `None`/`Err`, exactly as in Rust, and composes across
the host boundary (host errors arrive as `Err`):

```rust
fn parse_double(s: string) -> Result[int, string] {
    let Some(n) = s.parse_int() else { return Err("bad number") }
    Ok(n * 2)
}

fn sum(a: string, b: string) -> Result[int, string] {
    Ok(parse_double(a)? + parse_double(b)?)
}
```

Methods: `is_some is_none unwrap unwrap_or expect` /
`is_ok is_err unwrap unwrap_or unwrap_err expect`.

## Containers

```rust
let xs = [1, 2, 3]                 // List[int]
xs.push(4)
xs[0]                              // faults if out of bounds
xs.get(99)                         // Option[int] — never faults
xs.map(|x| x * 2).filter(|x| x > 2).fold(0, |a, x| a + x)

let ages = #{ "alice": 30, "bob": 25 }   // Map[string, int]
ages["carol"] = 22                 // insert or overwrite
ages["nope"]                       // faults — use .get for Option
```

Map keys are `int`, `bool`, `char` or `string`. List methods:
`len is_empty push pop get set insert remove clear contains index_of
reverse sort join map filter fold first last slice concat clone`.
Map methods: `len is_empty insert remove get contains_key keys values
clear clone`.

Strings are immutable; all operations are methods returning new strings.
`len`, `slice`, `find` work in characters (not bytes — `bytes_len` exists):
`len bytes_len is_empty split trim trim_start trim_end to_upper to_lower
starts_with ends_with contains find replace repeat pad_left pad_right
chars slice parse_int parse_float`.

## Loops

```rust
for i in 0..10 { }        // range (exclusive); 0..=10 inclusive
for x in [1, 2, 3] { }    // list elements
for k in ages { }         // map keys
for c in "abc" { }        // chars
while cond { }
loop { if done { break } }
```

## Traits

Go-flavored interfaces with Rust syntax. Static dispatch when the concrete
type is known; `dyn Trait` for dynamic dispatch (implicit coercion at
typed boundaries):

```rust
trait Shape {
    fn area(self) -> float
}

impl Shape for Circle {
    fn area(self) -> float { 3.14159 * self.r * self.r }
}

fn describe(s: dyn Shape) -> string { str(s.area()) }   // vtable dispatch
let shapes: List[dyn Shape] = [Circle { r: 1.0 }, square]
```

No default method bodies and no trait inheritance in v1.

### Operators

Operator overloading goes through built-in traits: `Add Sub Mul Div Rem
Neg Eq Ord Display Index`.

`==` on structs/enums **requires** an `Eq` impl — there is no implicit
identity equality (use `same(a, b)` for reference identity):

```rust
#[derive(Eq, Ord, Display, Clone)]
struct Vec2 { x: int, y: int }

impl Add for Vec2 {
    fn add(self, other: Vec2) -> Vec2 {
        Vec2 { x: self.x + other.x, y: self.y + other.y }
    }
}
```

Derived `Eq`/`Ord` are structural; `Display` is debug-ish; `Clone` is a
deep copy. `Ord`'s custom form is `fn cmp(self, other: Self) -> int`
(-1/0/1); `Index` is read-only in v1.

## Memory: reference counting and weak references

wscript uses **pure reference counting. Reference cycles leak.** This is a
deliberate, documented trade — there is no cycle collector. Break cycles
with weak references:

```rust
struct Node { kids: List[Node], parent: Option[weak[Node]] }

let child = Node { kids: [], parent: Some(weak(parent)) }
match child.parent {
    Some(w) => match w.upgrade() {     // upgrade() -> Option[Node]
        Some(p) => println(p.name),
        None => println("parent dropped"),
    }
    None => {}
}
```

If you build a structure where children hold *strong* references to
parents and vice versa, that memory will not be reclaimed until the VM is
dropped. Use `weak[T]` for back-references.

## Faults

VM-level faults (index out of bounds, division by zero, `unwrap()` on
`None`, aliasing violations at the host boundary) are **trappable
errors** delivered to the host with a stack trace — script code never
catches them, and they never panic the host. Prefer the `Option`-returning
APIs (`xs.get(i)`, `m.get(k)`) where failure is expected.

`wscript run` renders a fault with the message, a source snippet at the fault
site, and a full script stack trace — one frame per call, innermost first,
each with its `file:line:col`:

```
Error: list index 10 out of bounds (len 3)
   ╭─[script.wscript:2:5]
 2 │     xs[10]
   │     ───┬──
   │        ╰──── fault raised here
───╯
stack trace (most recent call first):
  at inner  script.wscript:2:5
  at outer  script.wscript:6:5
  at main   script.wscript:11:5
```

Embedders get the same structure programmatically: `RuntimeError.trace` is
a `Vec<TraceFrame>` (each a `function` name and an optional source `span`),
innermost frame first, with `RuntimeError.span` mirroring the fault site.

## Modules

`use module` imports a host-registered module; `use module::item` imports
one item. Scripts are single files in v1 — script-to-script imports are
planned for v2. Registered types are ambient (no `use` needed for type
names). The prelude — always available — is: `print println str fmt same
weak int float`.

## What wscript does not have (v1)

By design: borrow checker, `&`/`&mut`, lifetimes, user-defined generics
(the built-in containers are special-cased), exceptions, async, threads
(one VM per thread), implicit conversions, truthiness, a cycle collector,
string interpolation (use `fmt`), compound assignment (`+=`), bitwise
operators, range values outside `for` headers.
