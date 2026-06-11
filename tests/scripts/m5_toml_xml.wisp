// toml + xml modules sharing the Value type (parse JSON-ish, emit TOML).
// expect: 8080
// expect: name = "wisp"
// expect: true
// expect: rect
// expect: 2
// expect: <shapes count="2"><shape kind="rect"/><shape kind="circle"/></shapes>

use toml
use xml
use json

fn main() -> int {
    let conf = toml::parse("port = 8080\nname = \"wisp\"").unwrap()
    println(conf.get("port").unwrap().as_int().unwrap())

    // formats compose: Value parsed from TOML re-emits as TOML
    let emitted = toml::to_string(conf).unwrap()
    println(emitted.split("\n").filter(|l| l.starts_with("name")).join(""))

    // XML mapping convention: @attrs, repeated children -> lists
    let doc = xml::parse("<shapes count=\"2\"><shape kind=\"rect\"/><shape kind=\"circle\"/></shapes>").unwrap()
    let shapes = doc.get("shapes").unwrap()
    println(shapes.get("@attrs").unwrap().get("count").is_some())
    let first = shapes.get("shape").unwrap().at(0).unwrap()
    println(first.get("@attrs").unwrap().get("kind").unwrap().as_string().unwrap())
    println(shapes.get("shape").unwrap().len())

    // round-trip back out
    println(xml::to_string(doc).unwrap())
    0
}
