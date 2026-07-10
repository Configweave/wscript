// Format specs inside interpolation holes are reserved, not supported.
// error: format specs are not supported in interpolation yet

fn main() {
    let x = 3.14159
    println("pi is {x:.2}")
}
