# regex module

Regular expressions (pure — always safe to register; invalid patterns fault). The pattern is always the first argument, the haystack the second.

| Function | Signature |
| --- | --- |
| `is_match` | `(pattern: string, text: string) -> bool` |
| `find` | `(pattern: string, text: string) -> Option[string]` — first match, or `None` |
| `find_all` | `(pattern: string, text: string) -> List[string]` — every non-overlapping match, in order |
| `replace` | `(pattern: string, text: string, with: string) -> string` — replaces every match; `$1`/`$name` expand capture groups |
| `captures` | `(pattern: string, text: string) -> Option[List[string]]` — capture groups of the first match: group 0 (whole match) first; non-participating groups are empty strings |
| `split` | `(pattern: string, text: string) -> List[string]` — split around every match |

```rust
use regex
let words = regex::find_all("[a-z]+", "one, two, three")   // ["one", "two", "three"]
let masked = regex::replace("(\\d{4})\\d+", "4111111111", "$1…")
```

## Related

- [Modules & use](../references/concept_modules.md)

- [Strings](../references/concept_strings.md)

[← Back to SKILL.md](../SKILL.md)
