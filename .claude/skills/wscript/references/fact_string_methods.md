# string methods

Strings are immutable; every method returns a new value. `len`, `slice`, `find` are character-indexed (`bytes_len` gives bytes).

| Group | Methods |
| --- | --- |
| Size | `len` `bytes_len` `is_empty` |
| Split/join | `split` `join` |
| Trim | `trim` `trim_start` `trim_end` |
| Case | `to_upper` `to_lower` |
| Search | `starts_with` `ends_with` `contains` `find` |
| Reshape | `replace` `repeat` `pad_left` `pad_right` |
| Slice | `chars` `slice` |
| Parse | `parse_int` `parse_float` (return `Option`) |

## Related

- [Strings](../references/concept_strings.md)

[← Back to SKILL.md](../SKILL.md)
