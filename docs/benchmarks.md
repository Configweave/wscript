# Benchmarks

Microbenchmarks against the languages wisp competes with for the
embedding niche (PRD §10 M7). Honest numbers, whatever they are.

Reproduce with:

```sh
cargo run --release -p wisp-benchmarks
```

Method: each engine runs an equivalent program; only execution is timed
(compilation excluded); best of 5 runs, release profile. Sources live in
`benchmarks/src/main.rs`.

Measured 2026-06-11 on Linux 7.0.10-2-cachyos, 13th Gen Intel(R) Core(TM) i7-13700H, rustc 1.92.0.

| benchmark | wisp | Lua 5.4 (mlua) | Rhai | Rune |
|---|---|---|---|---|
| fib(25) recursive | 26.60ms | 3.39ms | 71.66ms | 31.96ms |
| loop sum 1..=2,000,000 | 85.52ms | 6.24ms | 276.36ms | 240.64ms |
| string build (20k pieces) | 3.49ms | 3.36ms | 53.47ms | 14.33ms |
| list sort (10k ints) | 1.08ms | 1.96ms | 3.66ms | 2.29ms |

## Reading the numbers

- **Lua wins raw interpretation** (function calls, hot loops) by roughly
  3–13×. Its C interpreter has had thirty years of tuning; wisp v1 uses a
  plain-enum register VM with no NaN-boxing and no dispatch tricks
  (deliberately — PRD §5.2 says start simple, benchmark later).
- **wisp beats Rhai and Rune on every benchmark measured** — typically
  2–4× faster than Rune and 3–15× faster than Rhai. Static typing pays:
  the emitter selects typed instructions (`AddI`, `LtI`) where dynamic
  engines dispatch on runtime tags.
- Workloads dominated by builtin routines (string build, sort) run at
  effectively native speed in all engines that implement them natively;
  wisp's are Rust.

## Known headroom (v2 candidates, PRD §5.2)

NaN-boxed values, inline caching of call frames, fused
compare-and-branch instructions, and avoiding per-instruction frame
re-borrowing in the dispatch loop.
