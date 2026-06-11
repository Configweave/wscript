# The wisp standard library reference

Every module is opt-in at embed time (capability-style — nothing
ambient) and feature-gated in Cargo. The CLI (`wisp run`) enables all of
them. Scripts import with `use math`, then call `math::abs(x)`.

**Decided surface note (PRD §7):** string operations are *methods on the
`string` type* — there is no `string` module. The one chosen surface is
`s.len()`, `s.split(",")`, etc.; formatting is the prelude's
`fmt(template, args…)`. See the [language tour](tour.md) for the full
method lists of `string`, `List`, `Map`, `Option` and `Result`.

## Prelude (always available, no import)

| function | signature | notes |
|---|---|---|
| `print` / `println` | `(any)` / `(any?)` | `println()` prints a bare newline |
| `str` | `(any) -> string` | uses `Display` impls when present |
| `fmt` | `(string, any…) -> string` | `{}` placeholders; `{{`/`}}` escape; count checked at compile time for literal templates |
| `same` | `(T, T) -> bool` | reference identity (PRD §3.7) |
| `weak` | `(T) -> weak[T]` | reference types only |
| `int` | `(int\|float\|char) -> int` | float truncates; char gives the code point |
| `float` | `(int\|float) -> float` | |

## `math` — pure numerics (always safe)

Float functions (take/return `float`): `abs min max clamp floor ceil
round trunc sqrt pow exp ln log2 log10 sin cos tan asin acos atan atan2
sinh cosh tanh asinh acosh atanh lerp signum`.

Int variants (wisp has no overloading): `iabs imin imax iclamp isignum`.

Constants: `PI E TAU INF NAN`.

Randomness: `rand() -> float` uniform in `[0, 1)`;
`rand_range(a: int, b: int) -> int` uniform in `[a, b)` (returns `a` when
the range is empty). Dependency-free splitmix64, clock-seeded.

## `fs` — file I/O (capability: filesystem)

All fallible operations return `Result[..., string]`.

| function | signature |
|---|---|
| `read_to_string` | `(string) -> Result[string, string]` |
| `read_bytes` | `(string) -> Result[List[int], string]` |
| `write`, `append` | `(string, string) -> Result[unit, string]` |
| `exists`, `is_file`, `is_dir` | `(string) -> bool` |
| `list_dir` | `(string) -> Result[List[string], string]` (sorted names) |
| `create_dir`, `create_dir_all` | `(string) -> Result[unit, string]` |
| `copy`, `rename` | `(string, string) -> Result[unit, string]` |
| `remove_file`, `remove_dir` | `(string) -> Result[unit, string]` |
| `join` | `(string, string) -> string` |
| `parent`, `file_name`, `ext` | `(string) -> Option[string]` |

## `process` — commands & environment (capability: process control)

```rust
use process

let out = process::run("git", ["status", "--short"])?
// out: Output { status: int, stdout: string, stderr: string }
```

| function | signature |
|---|---|
| `run` | `(string, List[string]) -> Result[Output, string]` |
| `env` | `(string) -> Option[string]` |
| `set_env` | `(string, string)` |
| `cwd` | `() -> Result[string, string]` |
| `args` | `() -> List[string]` — arguments after the script path on the CLI |

## The shared `Value` type (json / toml / xml)

The three data formats parse into **one** dynamic enum so formats compose
(parse JSON, emit TOML). It is the single dynamically-typed escape hatch
in the language:

```rust
enum Value {
    Null, Bool(bool), Int(int), Float(float), String(string),
    List(List[Value]), Map(Map[string, Value]),
}
```

You can `match` on it like any enum, or use the accessor methods
(register `wisp_std::value()` for these; the CLI always does):

`get(key) -> Option[Value]` · `at(idx) -> Option[Value]` ·
`keys() -> List[string]` · `len() -> int` · `is_null() -> bool` ·
`as_bool/as_int/as_float/as_string/as_list/as_map -> Option[...]`
(`as_float` also accepts `Int`).

## `json`

| function | signature |
|---|---|
| `parse` | `(string) -> Result[Value, string]` |
| `to_string` | `(Value) -> string` (keys sorted — deterministic) |
| `to_string_pretty` | `(Value) -> string` |

## `toml`

| function | signature |
|---|---|
| `parse` | `(string) -> Result[Value, string]` (datetimes become strings) |
| `to_string` | `(Value) -> Result[string, string]` |
| `to_string_pretty` | `(Value) -> Result[string, string]` |

Serialization fails on `Null` anywhere and on non-map top levels — TOML
documents are tables.

## `xml` — with the documented mapping convention

XML does not fit a JSON-shaped value model. The convention (decided in
the PRD; lossy but predictable):

- a document is `Map { root_name: element }`;
- an element with attributes/children is a `Map`: attributes under
  `"@attrs"` (a `Map[string, Value]`), text content under `"#text"`;
- **repeated child elements collapse into a `List`** under their name;
- text-only elements are plain `String`s; empty elements are `Null`;
- comments, processing instructions and doctypes are dropped.

```rust
let doc = xml::parse("<shapes count=\"2\"><shape kind=\"rect\"/><shape kind=\"circle\"/></shapes>")?
let shapes = doc.get("shapes").unwrap()
shapes.get("@attrs").unwrap().get("count")     // Some("2")
shapes.get("shape").unwrap().len()             // 2 — repeated → List
```

| function | signature |
|---|---|
| `parse` | `(string) -> Result[Value, string]` |
| `to_string` | `(Value) -> Result[string, string]` (needs exactly one root key) |
| `to_string_pretty` | `(Value) -> Result[string, string]` |

## Cargo features

`wisp-std` features (default = all): `math`, `fs`, `process`, `json`,
`toml`, `xml`. The shared `Value` type and its module are always
compiled. Embedders wanting a minimal build:

```toml
wisp-std = { version = "...", default-features = false, features = ["math"] }
```
