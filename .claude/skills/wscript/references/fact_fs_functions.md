# fs module

File I/O (capability: filesystem). All fallible operations return `Result[…, string]`.

| Function | Signature |
| --- | --- |
| `read_to_string` | `(string) -> Result[string, string]` |
| `read_bytes` | `(string) -> Result[List[int], string]` |
| `write`, `append` | `(string, string) -> Result[unit, string]` |
| `exists`, `is_file`, `is_dir` | `(string) -> bool` |
| `list_dir` | `(string) -> Result[List[string], string]` (sorted) |
| `create_dir`, `create_dir_all` | `(string) -> Result[unit, string]` |
| `copy`, `rename` | `(string, string) -> Result[unit, string]` |
| `remove_file`, `remove_dir` | `(string) -> Result[unit, string]` |
| `join` | `(string, string) -> string` |
| `parent`, `file_name`, `ext` | `(string) -> Option[string]` |

## Related

- [Modules & use](../references/concept_modules.md)

[← Back to SKILL.md](../SKILL.md)
