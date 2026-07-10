// Identity short-circuits before structural recursion: a cyclic value
// compared with itself is true (Rc pointer equality), no fault. Acyclic
// values with weak back-references still print fine.
// expect: true
// expect: child

#[derive(Eq)]
struct Node { kids: List[Node] }

struct Tree { name: string, kids: List[Tree], parent: Option[weak[Tree]] }

fn main() {
    let n = Node { kids: [] }
    n.kids.push(n)
    let alias = n
    println(n == alias)

    // weak backref pattern stays printable (no strong cycle).
    let parent = Tree { name: "parent", kids: [], parent: None }
    let child = Tree { name: "child", kids: [], parent: Some(weak(parent)) }
    parent.kids.push(child)
    println(parent.kids[0].name)
}
