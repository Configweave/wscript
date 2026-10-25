# Option & Result methods

| Type | Methods |
| --- | --- |
| `Option[T]` | `is_some` `is_none` `unwrap` `unwrap_or` `expect` |
| `Result[T, E]` | `is_ok` `is_err` `unwrap` `unwrap_or` `unwrap_err` `expect` |

`?` early-returns the `None`/`Err`. `unwrap()` on the empty case raises a trappable fault (not a host panic). `expect(msg)` faults with `msg` on `None`, and with `msg: <err>` on `Err`, so the payload survives.

## Related

- [Option, Result & ?](../references/concept_options_results.md)

[← Back to SKILL.md](../SKILL.md)
