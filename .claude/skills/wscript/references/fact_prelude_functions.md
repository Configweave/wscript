# Prelude (always available, no import)

| Function | Signature | Notes |
| --- | --- | --- |
| `print` / `println` | `(any)` / `(any?)` | `println()` prints a bare newline |
| `str` | `(any) -> string` | uses `Display` impls when present |
| `fmt` | `(string, any…) -> string` | `{}` placeholders; `{{`/`}}` escape; count and specs checked at compile time for literal templates |
| `same` | `(T, T) -> bool` | reference identity |
| `weak` | `(T) -> weak[T]` | reference types only |
| `int` | `(int\\|float\\|char) -> int` | float truncates; char gives the code point |
| `float` | `(int\\|float) -> float` |  |

`fmt` placeholders take a format spec: `{:[[fill]align][0][width][.prec][type]}` with align `<` `^` `>` and int-only types `x` `X` `b` `o`. Width/precision count characters; `0` zero-pads numbers sign-aware; precision means digits for floats and truncation for strings. Examples: `{:>8}` right-align in 8, `{:.2}` two decimals, `{:04}` zero-pad, `{:x}` hex. Interpolated string literals (`"n = {n}"`) take expressions only — a `:` spec inside a hole is rejected; reach for `fmt` when you need formatting.

## Related

- [Modules & use](../references/concept_modules.md)

- [Strings](../references/concept_strings.md)

[← Back to SKILL.md](../SKILL.md)
