# ADR-0001: `LocalId` is a frame slot index, never reused

- **Status:** accepted
- **Date:** 2026-08-01
- **Context:** [#3](https://github.com/Configweave/wscript/issues/3) (deepening programme), raised while designing [#8](https://github.com/Configweave/wscript/issues/8)

## Context

The checker allocates a `LocalId` per binding via `declare_local`
(`wscript-compiler/src/check/mod.rs:2272-2285`):

```rust
let state = self.fn_states.last_mut().unwrap();
let local = state.n_locals;
state.n_locals += 1;
```

The counter is monotonic per function, and `pop_scope`
(`check/mod.rs:2268-2270`) drops a scope's *names* but not its *slots*. So two
sibling blocks each declaring `let x` get distinct `LocalId`s, and a function's
frame is sized by its **total** local count rather than its peak live count.

This looks like an oversight — a straightforward liveness analysis would let
sibling scopes share slots and shrink frames. It was raised as a candidate
improvement while extracting the scope state into its own module.

## Decision

**Keep the monotonic allocation. `LocalId` is a frame slot index and is never
reused.** Record it as an invariant rather than leaving it an accidental
property.

## Rationale

Two things depend on it, neither of them local to the checker:

1. **The emitter identifies registers with locals.** `local_reg` is the identity
   function (`wscript-compiler/src/emit.rs:374-376`):

   ```rust
   fn local_reg(&self, local: LocalId) -> u16 { local as u16 }
   ```

   There is no mapping layer. A `LocalId` *is* a register index in the frame
   layout `[0..n_locals)` locals, `[n_locals..n_locals + n_caps)` capture cells,
   temps above.

2. **Capture tracking assumes a `LocalId` names exactly one binding.**
   `FnState::captured` is a `HashSet<LocalId>` (`check/mod.rs:348-356`) and
   drives whether a local is cell-allocated. If two sibling bindings shared a
   slot and only one were captured, the set would mark both — silently boxing an
   uncaptured local, or worse, aliasing a captured local's cell with an
   unrelated sibling's value.

The cost is frame size in long functions with many sequential blocks. That is
memory on a heap-allocated register stack, not a correctness or throughput
concern, and no benchmark has flagged it.

## Consequences

- Frames are sized by total locals. `n_regs` (`FnProto`) grows with the number of
  bindings in a function, not with peak liveness.
- Any future attempt to reuse slots must first replace `captured:
  HashSet<LocalId>` with something that distinguishes bindings, and introduce a
  real local→register mapping in the emitter. Both are in scope for a
  register-allocation change ([#12](https://github.com/Configweave/wscript/issues/12)),
  neither is in scope for scope-state extraction.
- If frame size ever does become a problem, the cheaper move is to track a
  peak-live count alongside `n_locals` so the emitter can size frames tighter
  **without** changing `LocalId` identity.
