// for over ranges, lists, maps (keys), strings (chars), and `continue` /
// `break` inside each shape of `for`.
// expect: 10
// expect: 15
// expect: a-b-c
// expect: k1 k2
// expect: 3
// expect: 1,3,5,7
// expect: a-b

fn main() {
    let total = 0
    for i in 0..5 { total = total + i }
    println(total)

    let total2 = 0
    for i in 1..=5 { total2 = total2 + i }
    println(total2)

    let parts = []
    for s in ["a", "b", "c"] { parts.push(s) }
    println(parts.join("-"))

    let m = #{ "k1": 1, "k2": 2 }
    let keys = []
    for k in m { keys.push(k) }
    println(keys.join(" "))

    let n = 0
    for c in "abc" { n = n + 1 }
    println(n)

    // A `continue` in a counted loop still runs the step — skipping it
    // would spin forever — and a `break` leaves before the range does.
    let odds = []
    for i in 0..10 {
        if i % 2 == 0 { continue }
        if i > 7 { break }
        odds.push(str(i))
    }
    println(odds.join(","))

    // The same two, over a list.
    let kept = []
    for s in ["a", "skip", "b", "stop", "c"] {
        if s == "skip" { continue }
        if s == "stop" { break }
        kept.push(s)
    }
    println(kept.join("-"))
}
