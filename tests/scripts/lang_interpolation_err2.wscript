// An unterminated interpolation hole is a loud compile error with a
// migration hint.
// error: unterminated `{` interpolation hole

fn main() {
    println("open {brace")
}
