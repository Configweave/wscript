// Structural == on distinct cyclic values is a trappable fault, not a
// process crash (the depth guard in the VM's structural ops).
// error: nesting too deep

#[derive(Eq)]
struct Node { kids: List[Node] }

fn cyc() -> Node {
    let n = Node { kids: [] }
    n.kids.push(n)
    n
}

fn main() {
    println(cyc() == cyc())
}
