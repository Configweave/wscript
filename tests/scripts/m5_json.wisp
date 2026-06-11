// json module + shared Value type.
// expect: wil
// expect: 2
// expect: 10
// expect: true
// expect: {"list":[1,2,3],"name":"wil"}

use json

fn main() -> int {
    let v = json::parse("{\"name\": \"wil\", \"list\": [1, 2, 3], \"hp\": 10}").unwrap()
    println(v.get("name").unwrap().as_string().unwrap())
    println(v.get("list").unwrap().at(1).unwrap().as_int().unwrap())
    println(v.get("hp").unwrap().as_int().unwrap())
    println(v.get("missing").is_none())

    // compose: build a Value and emit it
    let out = Value::Map(#{
        "name": Value::String("wil"),
        "list": Value::List([Value::Int(1), Value::Int(2), Value::Int(3)]),
    })
    println(json::to_string(out))
    0
}
