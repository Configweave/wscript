// regex module: is_match, find, find_all, replace, captures, split.
// expect: true
// expect: false
// expect: 42
// expect: none
// expect: ["12", "34", "56"]
// expect: X-X-X
// expect: ["05/01/2026"]
// expect: 2026-01-05
// expect: 2026
// expect: 01
// expect: ["a", "b", "c"]

use regex

fn main() -> int {
    println(regex::is_match("^h.llo$", "hello"))
    println(regex::is_match("^h.llo$", "help"))

    match regex::find("[0-9]+", "answer: 42!") {
        Some(m) => println(m),
        None => println("none"),
    }
    match regex::find("[0-9]+", "no digits") {
        Some(m) => println(m),
        None => println("none"),
    }

    println(regex::find_all("[0-9]+", "12 34 56"))

    println(regex::replace("[0-9]+", "1-2-3", "X"))
    // capture-group expansion in the replacement
    println([regex::replace("([0-9]{{4}})-([0-9]{{2}})-([0-9]{{2}})", "2026-01-05", "$3/$2/$1")])

    match regex::captures("([0-9]{{4}})-([0-9]{{2}})-([0-9]{{2}})", "on 2026-01-05 we ship") {
        Some(groups) => {
            println(groups[0])
            println(groups[1])
            println(groups[2])
        }
        None => println("none"),
    }

    println(regex::split(", *", "a, b,  c"))
    0
}
