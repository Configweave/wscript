# Map methods

Map literals are written `#{ k: v, … }`. Keys are `int`, `bool`, `char` or `string`.

| Group | Methods |
| --- | --- |
| Size | `len` `is_empty` `clear` |
| Add/remove | `insert` `remove` |
| Access | `get` (Option) `contains_key` `[k]` (faults if missing) |
| Iterate | `keys` `values` `each(\|k, v\| …)` |
| Transform | `map(\|k, v\| …) -> List[U]` `filter(\|k, v\| …) -> Map[K, V]` |
| Copy | `clone` |

## Related

- [Containers: List & Map](../references/concept_containers.md)

[← Back to SKILL.md](../SKILL.md)
