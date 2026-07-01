# Functions & Closures

_Signatures require annotations; blocks are expressions; closures capture by reference._

Annotations are **required on function signatures and nowhere else** — this is what
keeps inference local and compile errors readable. A block evaluates to its last
expression. An omitted return type means `unit`.


## Functions

```rust
fn area(w: int, h: int) -> int {
    w * h                  // blocks evaluate to their last expression
}

fn log(msg: string) {      // omitted return type = unit
    println(msg)
}
```

A trailing `;` discards a block's tail value (Rust-style). A function with no return type must not end on a non-unit expression — write `f();` to discard it.

## Function values & closures

```rust
fn apply(f: fn(int) -> int, x: int) -> int { f(x) }

fn make_counter() -> fn() -> int {
    let n = 0
    || { n = n + 1; n }    // closures capture by reference
}

let double = |x| x * 2     // params inferred where context determines them
apply(double, 21)          // 42
```

The function type `fn(T1, T2, …) -> R` describes a callable; use it for callback parameters. Closures (`|params| body`) capture their environment by reference.

## Examples

### Fibonacci — recursion, iteration, control flow

The M1 gate. Two functions, an `if`-expression body, a `while` loop, and `main` returning an exit code.

```rust
fn fib(n: int) -> int {
    if n < 2 { n } else { fib(n - 1) + fib(n - 2) }
}

fn fib_iter(n: int) -> int {
    let a = 0
    let b = 1
    let i = 0
    while i < n {
        let next = a + b
        a = b
        b = next
        i = i + 1
    }
    a
}

fn main() -> int {
    println(fib(20))        // 6765
    println(fib_iter(20))   // 6765
    if fib(20) == fib_iter(20) { 0 } else { 1 }
}
```

**Expected:** Prints 6765 twice; exits 0.

## Related

- [Control Flow](../references/concept_control_flow.md)

- [Values & Types](../references/concept_values.md)

- [The Type System](../references/concept_type_system.md)

[← Back to SKILL.md](../SKILL.md)
