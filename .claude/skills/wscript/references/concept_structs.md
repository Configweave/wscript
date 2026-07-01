# Structs & Methods

_Named field records with impl blocks; self is implicit and always by reference._

A `struct` is a named record of typed fields. Methods are declared in an `impl` block. `self` is implicit in type and **always by reference** — there is no `&` in wscript.

```rust
struct Player { name: string, hp: int }

impl Player {
    fn heal(self, amount: int) {
        self.hp = self.hp + amount
    }
}

let p = Player { name: "wil", hp: 100 }
p.heal(20)                 // p.hp == 120
```

> [!NOTE]
> **Construction & mutation**
> Construct with `Type { field: value, … }`. Fields are mutated in place (`p.hp = …`) — and because structs are reference types, that mutation is visible through every alias. See Reference Semantics.

## Examples

### Task list — structs, enums, match, Option/Result, containers

The M2 gate. A `Store` of `Task`s driven by `Command` variants; note `let … else`, `?`-free explicit error returns, closures in `filter`, and reference-semantics mutation through `self`.

```rust
struct Task { id: int, title: string, done: bool, tags: List[string] }

enum Command { Add(string), Done(int), Remove(int), Report }

struct Store { tasks: List[Task], next_id: int }

impl Store {
    fn add(self, title: string) -> int {
        let id = self.next_id
        self.next_id = self.next_id + 1
        let tags = []
        for word in title.split(" ") {
            if word.starts_with("#") { tags.push(word.slice(1, word.len())) }
        }
        self.tasks.push(Task { id: id, title: title, done: false, tags: tags })
        id
    }

    fn complete(self, id: int) -> Result[string, string] {
        let Some(task) = self.find(id) else { return Err(fmt("no task with id {}", id)) }
        task.done = true
        Ok(task.title)
    }

    fn find(self, id: int) -> Option[Task] {
        for t in self.tasks { if t.id == id { return Some(t) } }
        None
    }

    fn report(self) {
        let open = self.tasks.filter(|t| !t.done)
        println(fmt("{} open / {} done", open.len(), self.tasks.len() - open.len()))
    }
}
```

**Expected:** Drives the store with a list of Command variants and prints a report.

## Related

- [Enums](../references/concept_enums.md)

- [Traits](../references/concept_traits.md)

- [Reference Semantics](../references/concept_reference_semantics.md)

[← Back to SKILL.md](../SKILL.md)
