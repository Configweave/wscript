// fs module: write, read, list, paths, cleanup.
// expect: hello wscript
// expect: hello wscript!
// expect: true
// expect: m5_fs_test.txt
// expect: txt
// expect: false

use fs

fn main() -> int {
    let dir = "/tmp/wscript_m5_fs"
    fs::create_dir_all(dir).unwrap()
    let path = fs::join(dir, "m5_fs_test.txt")
    fs::write(path, "hello wscript").unwrap()
    println(fs::read_to_string(path).unwrap())
    fs::append(path, "!").unwrap()
    println(fs::read_to_string(path).unwrap())
    println(fs::exists(path) && fs::is_file(path) && fs::is_dir(dir))
    for name in fs::list_dir(dir).unwrap() {
        println(name)
    }
    match fs::ext(path) {
        Some(e) => println(e),
        None => println("none"),
    }
    fs::remove_file(path).unwrap()
    println(fs::exists(path))
    fs::remove_dir(dir).unwrap()
    0
}
