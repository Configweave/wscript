# The shared Value type

The single dynamically-typed escape hatch — the parse target for json/toml/xml. `match` on it like any enum, or use the accessor methods (register `wscript_std::value()`; the CLI always does).

```rust
enum Value {
    Null, Bool(bool), Int(int), Float(float), String(string),
    List(List[Value]), Map(Map[string, Value]),
}
```

| Method | Returns |
| --- | --- |
| `get(key)` | `Option[Value]` |
| `at(idx)` | `Option[Value]` |
| `keys()` | `List[string]` |
| `len()` | `int` |
| `is_null()` | `bool` |
| `as_bool/as_int/as_float/as_string/as_list/as_map` | `Option[...]` (`as_float` also accepts `Int`) |

## Related

- [json / toml / xml modules](../references/fact_data_formats.md)

[← Back to SKILL.md](../SKILL.md)
