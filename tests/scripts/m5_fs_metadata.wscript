// fs metadata: size, mtime_unix, walk.
// expect: 5
// expect: true
// expect: /tmp/wscript_m5_fsmeta/a.txt
// expect: /tmp/wscript_m5_fsmeta/sub
// expect: /tmp/wscript_m5_fsmeta/sub/b.txt

use fs

fn main() -> int {
    let dir = "/tmp/wscript_m5_fsmeta"
    fs::create_dir_all(fs::join(dir, "sub")).unwrap()
    let a = fs::join(dir, "a.txt")
    let b = fs::join(fs::join(dir, "sub"), "b.txt")
    fs::write(a, "12345").unwrap()
    fs::write(b, "x").unwrap()

    println(fs::size(a).unwrap())
    println(fs::mtime_unix(a).unwrap() > 1.0e9)
    for path in fs::walk(dir).unwrap() {
        println(path)
    }

    fs::remove_file(a).unwrap()
    fs::remove_file(b).unwrap()
    fs::remove_dir(fs::join(dir, "sub")).unwrap()
    fs::remove_dir(dir).unwrap()
    0
}
