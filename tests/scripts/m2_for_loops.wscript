// for over ranges, lists, maps (keys), strings (chars).
// expect: 10
// expect: 15
// expect: a-b-c
// expect: k1 k2
// expect: 3

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
}
