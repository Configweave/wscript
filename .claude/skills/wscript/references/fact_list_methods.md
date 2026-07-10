# List methods

| Group | Methods |
| --- | --- |
| Size | `len` `is_empty` `clear` |
| Add/remove | `push` `pop` `insert` `remove` |
| Access | `get` (Option) `set` `first` `last` `[i]` (faults out of bounds) |
| Search | `contains` `index_of` |
| Predicates | `any(\|x\| …)` `all(\|x\| …)` `count(\|x\| …)` |
| Find | `find(\|x\| …) -> Option[T]` `position(\|x\| …) -> Option[int]` |
| Reorder | `reverse` `sort` `sort_by(\|a, b\| …)` (comparator returns `int`: negative, zero, positive) |
| Transform | `map` `filter` `fold` `join` `slice` `concat` |
| Indexed / paired | `map_indexed(\|i, x\| …)` `zip_with(other, \|a, b\| …)` (stops at the shorter list) |
| Aggregate | `sum` (numeric elements) `min` `max` (`Ord` elements, `Option` — `None` when empty) |
| Copy | `clone` |

## Related

- [Containers: List & Map](../references/concept_containers.md)

[← Back to SKILL.md](../SKILL.md)
