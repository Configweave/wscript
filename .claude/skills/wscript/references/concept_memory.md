# Memory: Reference Counting & weak\[T\]

_Pure reference counting; cycles leak by design; break them with weak references._

wscript uses **pure reference counting. Reference cycles leak.** This is a deliberate,
documented trade — there is no cycle collector. Break cycles with weak references.


```rust
struct Node { kids: List[Node], parent: Option[weak[Node]] }

let child = Node { kids: [], parent: Some(weak(parent)) }
match child.parent {
    Some(w) => match w.upgrade() {     // upgrade() -> Option[Node]
        Some(p) => println(p.name),
        None => println("parent dropped"),
    }
    None => {}
}
```

`weak(x)` (prelude) makes a `weak[T]` from a reference value; `w.upgrade()` returns `Option[T]` — `None` once the strong references are gone. If children hold \*strong\* references to parents and vice versa, that memory is not reclaimed until the VM is dropped; use `weak[T]` for back-references.

## Related

- [Reference Semantics](../references/concept_reference_semantics.md)

- [Structs & Methods](../references/concept_structs.md)

- [Faults](../references/concept_faults.md)

[← Back to SKILL.md](../SKILL.md)
