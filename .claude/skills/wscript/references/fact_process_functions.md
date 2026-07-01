# process module

Commands & environment (capability: process control).

| Function | Signature |
| --- | --- |
| `run` | `(string, List[string]) -> Result[Output, string]` |
| `env` | `(string) -> Option[string]` |
| `set_env` | `(string, string)` |
| `cwd` | `() -> Result[string, string]` |
| `args` | `() -> List[string]` — arguments after the script path on the CLI |

`Output` is `{ status: int, stdout: string, stderr: string }`.

```rust
use process
let out = process::run("git", ["status", "--short"])?
```

## Related

- [Modules & use](../references/concept_modules.md)

[← Back to SKILL.md](../SKILL.md)
