// Non-exhaustive match is a compile error with a witness.
// error: non-exhaustive match

enum Dir { North, South, East, West }

fn main() {
    let d = Dir::East
    match d {
        Dir::North => println("n"),
        Dir::South => println("s"),
    }
}
