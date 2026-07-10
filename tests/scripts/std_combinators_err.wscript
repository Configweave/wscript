// sum over non-numeric elements is a compile error (E0242).
// error: requires int or float elements

fn main() {
    println(["a", "b"].sum())
}
