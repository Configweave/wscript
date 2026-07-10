// Operations a bound doesn't grant are rejected with a pointed help.
// error: `==` on `T` requires an `Eq` bound

fn eq_them[T](a: T, b: T) -> bool { a == b }

fn main() {}
