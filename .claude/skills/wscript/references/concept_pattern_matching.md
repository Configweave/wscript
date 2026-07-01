# Pattern Matching

_match is an expression, exhaustiveness-checked at compile time, with guards, or-patterns and destructuring._

`match` is an expression and is **exhaustiveness-checked at compile time** — a missing
variant is a compile error with a witness. Exhaustiveness analysis is exact for nested
variant / struct / bool / literal patterns.


```rust
fn handle(e: Event) -> bool {
    match e {
        Event::Quit => false,
        Event::Key(c) if c == 'q' => false,            // guards
        Event::Key('h') | Event::Key('?') => help(),   // or-patterns, literals
        Event::Key(_) => true,
        Event::Click { x, y } => x >= 0 && y >= 0,     // struct destructuring
    }
}
```

Arms with `if` guards never count toward exhaustiveness. Binding inside an or-pattern is not supported in v1.

### Patterns

| Pattern | Matches |
| --- | --- |
| `_` | anything (wildcard) |
| `name` | anything, binding it to `name` |
| literal (`42`, `'q'`, `"hi"`) | equality with a literal |
| `Enum::Variant(x)` | tuple variant, binding the payload |
| `Enum::Variant { x, y }` | struct variant, destructuring fields |
| `pat1 \| pat2` | either pattern (no bindings) |

## if let / let else

`if let` and `let … else` work as in Rust; a `let-else` block must diverge (return/break/loop).

```rust
if let Some(task) = store.find(id) { task.done = true }

let Some(n) = s.parse_int() else { return Err("not a number") }
```

## Related

- [Enums](../references/concept_enums.md)

- [Option, Result & ?](../references/concept_options_results.md)

- [Control Flow](../references/concept_control_flow.md)

[← Back to SKILL.md](../SKILL.md)
