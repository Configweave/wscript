# Faults

_Trappable VM-level errors delivered to the host with a stack trace; never panics, never caught by script._

VM-level **faults** — index out of bounds, division by zero, `unwrap()` on `None`,
aliasing violations at the host boundary — are trappable errors delivered to the host
with a stack trace. Script code never catches them, and they never panic the host.
Prefer the `Option`-returning APIs where failure is expected.


## What a fault looks like

`wscript run` renders a fault with the message, a source snippet at the fault site, and a full script stack trace (one frame per call, innermost first, each with `file:line:col`):

```text
Error: list index 10 out of bounds (len 3)
   ╭─[script.wscript:2:5]
 2 │     xs[10]
   │     ───┬──
   │        ╰──── fault raised here
───╯
stack trace (most recent call first):
  at inner  script.wscript:2:5
  at outer  script.wscript:6:5
  at main   script.wscript:11:5
```

Embedders get the same structure programmatically: `RuntimeError.trace` is a `Vec<TraceFrame>` (each a function name and an optional source span), innermost first, with `RuntimeError.span` mirroring the fault site. `vm.call*` returns `Err(Error::Runtime(_))` for faults.

## Related

- [Option, Result & ?](../references/concept_options_results.md)

- \[Memory: Reference Counting & weak\[T\]\](concept_memory)

- [Embedding in Rust](../references/concept_embedding.md)

[← Back to SKILL.md](../SKILL.md)
