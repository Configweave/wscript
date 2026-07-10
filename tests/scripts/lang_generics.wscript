// Generic functions (stage 1): erasure, Eq/Ord/Clone bounds, inference
// from arguments and expected types, closures over T, recursion,
// generic-calls-generic.
// expect: 42
// expect: hi
// expect: 9
// expect: 2.5
// expect: zebra
// expect: (3, 4)
// expect: first: 1
// expect: none
// expect: true
// expect: false
// expect: cloned: [1, 2] then [1, 2, 3]
// expect: 10
// expect: [7, 7, 7]
// expect: 6

#[derive(Eq, Ord, Display)]
struct P { x: int, y: int }

impl Display for P {
    fn fmt(self) -> string { "({self.x}, {self.y})" }
}

fn identity[T](x: T) -> T { x }

fn max_of[T: Ord](a: T, b: T) -> T {
    if a > b { a } else { b }
}

fn head[T](xs: List[T]) -> Option[T] { xs.first() }

fn dup[T: Clone](x: T) -> T { x.clone() }

fn has[T: Eq](xs: List[T], x: T) -> bool { xs.contains(x) }

fn apply[T, U](f: fn(T) -> U, x: T) -> U { f(x) }

fn rep[T](n: int, x: T) -> List[T] {
    let out: List[T] = []
    let i = 0
    while i < n {
        out.push(x)
        i += 1
    }
    out
}

// generic calling generic: the caller's bound covers the callee's.
fn largest3[T: Ord](a: T, b: T, c: T) -> T {
    max_of(max_of(a, b), c)
}

fn main() -> int {
    println(identity(42))
    println(identity("hi"))

    println(max_of(9, 3))
    println(max_of(1.5, 2.5))
    println(max_of("apple", "zebra"))
    println(max_of(P { x: 1, y: 2 }, P { x: 3, y: 4 }))   // derived Ord

    match head([1, 2, 3]) {
        Some(v) => println("first: {v}"),
        None => println("none"),
    }
    let empty: List[int] = []
    match head(empty) {
        Some(v) => println("first: {v}"),
        None => println("none"),
    }

    println(has(["a", "b"], "a"))
    println(has([1, 2], 3))

    // Clone bound: deep copy, mutation invisible to the copy.
    let orig = [1, 2]
    let copy = dup(orig)
    orig.push(3)
    println("cloned: {copy} then {orig}")

    // closure arg: the int argument pins T before the closure checks.
    println(apply(|v| v * 2, 5))

    println(rep(3, 7))

    println(largest3(4, 6, 5))
    0
}
