# Control Flow

_if/while/loop/for and statement-ends-at-newline; if and match are expressions._

`if` is an expression; its branches must agree on a type. Loops are `while`, `loop`, and `for x in iterable`.

```rust
let sign = if x < 0 { -1 } else if x > 0 { 1 } else { 0 }

for i in 0..10 { }        // range (exclusive); 0..=10 is inclusive
for x in [1, 2, 3] { }    // list elements
for k in ages { }         // map keys
for c in "abc" { }        // chars
while cond { }
loop { if done { break } }
```

> [!NOTE]
> **Ranges are header-only**
> `0..10` / `0..=10` are valid only in a `for` header — there are no first-class range values in v1.

## Statements end at newlines

Semicolons are permitted, never required. A statement continues across a newline only
when the line obviously cannot end: inside an unclosed `(` or `[`, just after a token
that cannot end an expression (a binary operator, `,`, `=`), when the next line starts
with `.`, or when the next token is `else`.


```rust
let total = add(1,
    2, 3)              // open ( — newlines don't terminate inside ( or [

let s = "hello"
    .to_upper()        // a line starting with `.` continues the chain

if total > 5 { println("big") }
else { println("small") }   // `else` may start a line
```

## Related

- [Functions & Closures](../references/concept_functions.md)

- [Pattern Matching](../references/concept_pattern_matching.md)

- [Containers: List & Map](../references/concept_containers.md)

[← Back to SKILL.md](../SKILL.md)
