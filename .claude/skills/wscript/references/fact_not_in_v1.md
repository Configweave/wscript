# Not in wscript v1 (by design)

Deliberately absent in v1:

| Absent | Instead |
| --- | --- |
| borrow checker, `&`/`&mut`, lifetimes | reference counting; `self` is implicitly by reference |
| user-defined generics | built-in `List[T]` / `Map[K,V]` only |
| exceptions | `Result` + `?`; trappable faults to the host |
| async, threads sharing values | one VM per thread (`Rc`, not `Arc`) |
| implicit conversions, truthiness | `int(x)`/`float(x)`; `bool` conditions |
| a cycle collector | cycles leak; use `weak[T]` |
| string interpolation | `fmt(template, args…)` |
| compound assignment (`+=`), bitwise operators | write the full expression |
| range values outside `for` headers | `0..n` only in a `for` header |
| script-to-script imports | single-file scripts (planned for v2) |

## Related

- [The Type System](../references/concept_type_system.md)

- \[Memory: Reference Counting & weak\[T\]\](concept_memory)

[← Back to SKILL.md](../SKILL.md)
