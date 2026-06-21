// Option/Result, ?, if let, let else, builtin methods.
// expect: found 9
// expect: missing
// expect: 10
// expect: err: bad number
// expect: 42

fn find(items: List[int], needle: int) -> Option[int] {
    let i = 0
    while i < items.len() {
        if items[i] == needle { return Some(i) }
        i = i + 1
    }
    None
}

fn parse_double(s: string) -> Result[int, string] {
    let Some(n) = s.parse_int() else {
        return Err("bad number")
    }
    Ok(n * 2)
}

fn sum_doubles(a: string, b: string) -> Result[int, string] {
    Ok(parse_double(a)? + parse_double(b)?)
}

fn main() {
    let items = [3, 5, 7, 9]
    if let Some(idx) = find(items, 9) {
        println("found " + str(items[idx]))
    }
    if find(items, 100).is_none() {
        println("missing")
    }
    match sum_doubles("2", "3") {
        Ok(n) => println(n),
        Err(e) => println("err: " + e),
    }
    match sum_doubles("2", "x") {
        Ok(n) => println(n),
        Err(e) => println("err: " + e),
    }
    println(Some(42).unwrap_or(0))
}
