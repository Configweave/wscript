// Map combinators: each, map, filter over (key, value) pairs. BTree
// ordering makes iteration deterministic.
// expect: a=1
// expect: b=2
// expect: c=3
// expect: ["a:1", "b:2", "c:3"]
// expect: #{"b": 2, "c": 3}
// expect: 3

fn main() -> int {
    let m = #{ "a": 1, "b": 2, "c": 3 }

    m.each(|k, v| println(fmt("{}={}", k, v)))

    println(m.map(|k, v| fmt("{}:{}", k, v)))

    println(m.filter(|k, v| v > 1))

    println(m.len())     // filter returned a new map; m is unchanged
    0
}
