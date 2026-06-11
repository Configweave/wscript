// M2 gate: a small task-list app — structs, enums, match, Option/Result,
// List/Map, string methods, for loops, closures.

struct Task {
    id: int,
    title: string,
    done: bool,
    tags: List[string],
}

enum Command {
    Add(string),
    Done(int),
    Remove(int),
    Report,
}

struct Store {
    tasks: List[Task],
    next_id: int,
}

impl Store {
    fn add(self, title: string) -> int {
        let id = self.next_id
        self.next_id = self.next_id + 1
        let tags = []
        for word in title.split(" ") {
            if word.starts_with("#") {
                tags.push(word.slice(1, word.len()))
            }
        }
        self.tasks.push(Task { id: id, title: title, done: false, tags: tags })
        id
    }

    fn find(self, id: int) -> Option[Task] {
        for t in self.tasks {
            if t.id == id { return Some(t) }
        }
        None
    }

    fn complete(self, id: int) -> Result[string, string] {
        let Some(task) = self.find(id) else {
            return Err(fmt("no task with id {}", id))
        }
        task.done = true
        Ok(task.title)
    }

    fn remove(self, id: int) -> Result[string, string] {
        let i = 0
        while i < self.tasks.len() {
            if self.tasks[i].id == id {
                let removed = self.tasks.remove(i)
                return Ok(removed.title)
            }
            i = i + 1
        }
        Err(fmt("no task with id {}", id))
    }

    fn report(self) {
        let open = self.tasks.filter(|t| !t.done)
        let closed = self.tasks.filter(|t| t.done)
        println(fmt("{} open / {} done", open.len(), closed.len()))
        for t in self.tasks {
            let mark = if t.done { "x" } else { " " }
            let tags = if t.tags.is_empty() {
                ""
            } else {
                "  [" + t.tags.join(", ") + "]"
            }
            println(fmt("[{}] {}. {}{}", mark, t.id, t.title, tags))
        }
    }
}

fn apply(store: Store, cmd: Command) {
    match cmd {
        Command::Add(title) => {
            let id = store.add(title)
            println(fmt("added task {}", id))
        }
        Command::Done(id) => {
            match store.complete(id) {
                Ok(title) => println(fmt("done: {}", title)),
                Err(e) => println("error: " + e),
            }
        }
        Command::Remove(id) => {
            match store.remove(id) {
                Ok(title) => println(fmt("removed: {}", title)),
                Err(e) => println("error: " + e),
            }
        }
        Command::Report => store.report(),
    }
}

fn main() -> int {
    let store = Store { tasks: [], next_id: 1 }
    let script = [
        Command::Add("write the parser #wisp"),
        Command::Add("water the plants #home"),
        Command::Add("emit bytecode #wisp"),
        Command::Done(1),
        Command::Remove(2),
        Command::Done(99),
        Command::Report,
    ]
    for cmd in script {
        apply(store, cmd)
    }
    0
}
