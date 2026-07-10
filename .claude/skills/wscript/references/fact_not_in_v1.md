# Not in wscript (by design)

Deliberately absent:

| Absent | Instead |
| --- | --- |
| borrow checker, `&`/`&mut`, lifetimes | reference counting; `self` is implicitly by reference |
| generic structs/enums/traits | generic \*functions\* (`fn f[T, U: Ord]`, type-erased) plus built-in `List[T]` / `Map[K,V]` |
| exceptions | `Result` + `?`; trappable faults to the host |
| async, threads sharing values | one VM per thread (`Rc`, not `Arc`) |
| implicit conversions, truthiness | `int(x)`/`float(x)`; `bool` conditions |
| a cycle collector | cycles leak; use `weak[T]` |
| bitwise operators | `math` helpers or write the arithmetic out |
| format specs in interpolation holes | `"{expr}"` only; use `fmt("{:.2}", x)` for formatting |
| range values outside `for` headers | `0..n` only in a `for` header |

## Related

- [The Type System](../references/concept_type_system.md)

- [Memory: Reference Counting & weak references](../references/concept_memory.md)

[← Back to SKILL.md](../SKILL.md)
