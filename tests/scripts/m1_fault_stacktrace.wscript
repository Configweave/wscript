// A fault deep in a call chain reports a full script stack trace with a
// per-frame source location, not just the innermost function name.
// error: stack trace (most recent call first):

fn inner(xs: List[int]) -> int {
    xs[10]
}

fn outer(xs: List[int]) -> int {
    inner(xs)
}

fn main() -> int {
    let xs = [1, 2, 3]
    outer(xs)
}
