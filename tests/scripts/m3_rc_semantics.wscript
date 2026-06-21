// Reference semantics suite (PRD M3): aliasing, mutation visibility
// through args/returns/containers, weak refs, cycles.
// expect: 99
// expect: 99
// expect: [9, 2]
// expect: alive
// expect: gone
// expect: parent

struct Node { name: string, kids: List[Node], parent: Option[weak[Node]] }

fn bump(p: Counter) { p.n = 99 }

struct Counter { n: int }

fn main() {
    // args alias the same object
    let c = Counter { n: 1 }
    bump(c)
    println(c.n)

    // returns alias too
    let c2 = identity(c)
    c2.n = 99
    println(c.n)

    // containers hold references
    let inner = [1, 2]
    let outer = [inner]
    inner[0] = 9
    println(outer[0])

    // weak refs: upgrade while alive, None after drop
    let node = Node { name: "root", kids: [], parent: None }
    let w = weak(node)
    match w.upgrade() {
        Some(n) => println("alive"),
        None => println("dead"),
    }
    drop_and_check()

    // cycle with weak back-reference (PRD §4.2 pattern)
    let parent = Node { name: "parent", kids: [], parent: None }
    let child = Node { name: "child", kids: [], parent: Some(weak(parent)) }
    parent.kids.push(child)
    match child.parent {
        Some(wp) => match wp.upgrade() {
            Some(p) => println(p.name),
            None => println("lost"),
        }
        None => println("none"),
    }
}

fn identity(c: Counter) -> Counter { c }

fn make_weak() -> weak[Counter] {
    let c = Counter { n: 5 }
    weak(c)
}

fn drop_and_check() {
    let w = make_weak()
    match w.upgrade() {
        Some(c) => println("still here"),
        None => println("gone"),
    }
}
