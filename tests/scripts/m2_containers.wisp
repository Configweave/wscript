// List/Map literals, indexing, methods, closures (map/filter/fold).
// expect: [1, 2, 3, 4]
// expect: 10
// expect: [2, 4]
// expect: [2, 4, 6, 8]
// expect: 3
// expect: alice=30 bob=25
// expect: true

fn main() {
    let xs = [1, 2, 3]
    xs.push(4)
    println(xs)
    println(xs.fold(0, |acc, x| acc + x))
    println(xs.filter(|x| x % 2 == 0))
    println(xs.map(|x| x * 2))

    let ages = #{ "alice": 30, "bob": 25 }
    ages["carol"] = 22
    println(ages.len())
    println(fmt("alice={} bob={}", ages["alice"], ages["bob"]))
    println(ages.get("dave").is_none())
}
