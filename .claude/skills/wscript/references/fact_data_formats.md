# json / toml / xml modules

Three data formats parse into one shared dynamic [`Value`](../references/fact_value_methods.md) type, so formats compose (parse JSON, emit TOML).

| Module | Functions |
| --- | --- |
| `json` | `parse(string) -> Result[Value, string]`; `to_string(Value) -> string` (keys sorted); `to_string_pretty` |
| `toml` | `parse(string) -> Result[Value, string]` (datetimes become strings); `to_string` / `to_string_pretty -> Result[string, string]` |
| `xml` | `parse(string) -> Result[Value, string]`; `to_string` / `to_string_pretty -> Result[string, string]` (needs exactly one root key) |

TOML serialization fails on `Null` anywhere and on non-map top levels. XML uses a documented mapping: attributes under `"@attrs"`, text under `"#text"`, repeated child elements collapse into a `List`.

## Examples

### Config tool — JSON in, TOML out via the shared Value

The M5 example. Parse JSON into a `Value`, read fields via the accessor methods, do some math, and re-emit as TOML — formats compose over one Value.

```rust
use json
use toml
use fs
use math

fn main() -> int {
    let raw = "{\"name\": \"demo\", \"port\": 8080, \"retries\": [1, 2, 3]}"
    let conf = json::parse(raw).unwrap()

    let name = conf.get("name").unwrap().as_string().unwrap_or("unnamed")
    let port = conf.get("port").unwrap().as_int().unwrap_or(80)
    println(fmt("service {} on port {}", name, port))

    let retries = conf.get("retries").unwrap()
    let total = 0
    for i in 0..retries.len() {
        total = total + retries.at(i).unwrap().as_int().unwrap_or(0)
    }
    println(fmt("total retry budget: {}s (sqrt {})", total, math::sqrt(float(total))))

    // formats compose: JSON in, TOML out
    println(toml::to_string(conf).unwrap().trim())

    let path = "/tmp/wscript_config_tool.toml"
    fs::write(path, toml::to_string(conf).unwrap()).unwrap()
    let ok = fs::exists(path)
    fs::remove_file(path).unwrap()
    if ok { 0 } else { 1 }
}
```

**Expected:** Prints the service line, the retry budget, and the TOML rendering of the config.

## Related

- [The shared Value type](../references/fact_value_methods.md)

- [Modules & use](../references/concept_modules.md)

[← Back to SKILL.md](../SKILL.md)
