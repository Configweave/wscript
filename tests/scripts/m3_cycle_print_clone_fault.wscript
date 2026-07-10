// Printing (derived Display) of a cyclic value is a trappable fault, not
// a process crash. Clone on cyclic data faults the same way (same depth
// guard, exercised in the VM unit tests).
// error: nesting too deep

#[derive(Display, Clone)]
struct Node { kids: List[Node] }

fn cyc() -> Node {
    let n = Node { kids: [] }
    n.kids.push(n)
    n
}

fn main() {
    println(cyc())
}
