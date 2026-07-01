# Prelude (always available, no import)

| Function | Signature | Notes |
| --- | --- | --- |
| `print` / `println` | `(any)` / `(any?)` | `println()` prints a bare newline |
| `str` | `(any) -> string` | uses `Display` impls when present |
| `fmt` | `(string, any…) -> string` | `{}` placeholders; `{{`/`}}` escape; count checked at compile time for literal templates |
| `same` | `(T, T) -> bool` | reference identity |
| `weak` | `(T) -> weak[T]` | reference types only |
| `int` | `(int\\|float\\|char) -> int` | float truncates; char gives the code point |
| `float` | `(int\\|float) -> float` |  |

## Related

- [Modules & use](../references/concept_modules.md)

- [Strings](../references/concept_strings.md)

[← Back to SKILL.md](../SKILL.md)
