# Benchmarks

Microbenchmarks against the languages wscript competes with for the
embedding niche (PRD §10 M7). Honest numbers, whatever they are.

Reproduce with:

```sh
cargo run --release -p wscript-benchmarks
```

Method: each engine runs an equivalent program; only execution is timed
(compilation excluded); best of 5 runs, release profile. Sources live in
`benchmarks/src/main.rs`.

Measured 2026-06-12 on Linux 7.0.10-2-cachyos, 13th Gen Intel(R) Core(TM) i7-13700H, rune 0.14.2, rustc 1.92.0.

| benchmark | wscript | Lua 5.4 (mlua) | Rhai | Rune |
|---|---|---|---|---|
| fib(25) recursive | 30.78ms | 3.86ms | 78.41ms | 13.06ms |
| loop sum 1..=2,000,000 | 94.49ms | 7.09ms | 316.11ms | 92.02ms |
| string build (20k pieces) | 4.26ms | 4.30ms | 62.26ms | 10.15ms |
| list sort (10k ints) | 1.37ms | 2.49ms | 4.62ms | 1.62ms |

## Reading the numbers

- **Lua wins raw interpretation** (function calls, hot loops) by roughly
  3–13×. Its C interpreter has had thirty years of tuning; wscript v1 uses a
  plain-enum register VM with no NaN-boxing and no dispatch tricks
  (deliberately — PRD §5.2 says start simple, benchmark later).
- **wscript beats Rhai on every benchmark** (3–15×).
- **Rune 0.14 and wscript trade blows**: Rune's tuned call path wins
  recursive fib (~2.4×); the loop is a wash; wscript wins the
  builtin-dominated workloads (string build ~2.4×, sort ~1.2×) where its
  statically-selected typed instructions and native routines pay off.
  (Rune 0.13 lost all four; their 0.14 release improved call performance
  substantially.)

## Known headroom (v2 candidates, PRD §5.2)

NaN-boxed values, inline caching of call frames, fused
compare-and-branch instructions, and avoiding per-instruction frame
re-borrowing in the dispatch loop.
