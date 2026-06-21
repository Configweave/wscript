// M5 example: stdlib in action — read JSON config, validate, emit TOML,
// report via process/fs.

use json
use toml
use fs
use math

fn main() -> int {
    let raw = "{\"name\": \"demo\", \"port\": 8080, \"retries\": [1, 2, 3]}"
    let conf = json::parse(raw).unwrap()

    let name = conf.get("name").unwrap().as_string().unwrap_or("unnamed")
    let port = conf.get("port").unwrap().as_int().unwrap_or(80)
    println(fmt("service {} on port {}", name, port))

    let retries = conf.get("retries").unwrap()
    let total = 0
    for i in 0..retries.len() {
        total = total + retries.at(i).unwrap().as_int().unwrap_or(0)
    }
    println(fmt("total retry budget: {}s (sqrt {})", total, math::sqrt(float(total))))

    // formats compose: JSON in, TOML out (PRD §7)
    println(toml::to_string(conf).unwrap().trim())

    let path = "/tmp/wscript_config_tool.toml"
    fs::write(path, toml::to_string(conf).unwrap()).unwrap()
    let ok = fs::exists(path)
    fs::remove_file(path).unwrap()
    if ok { 0 } else { 1 }
}
