// Enums (all three variant kinds), match exhaustiveness, guards, or-patterns.
// expect: quit
// expect: key x
// expect: clicked 3,4
// expect: small
// expect: big

enum Event {
    Quit,
    Key(char),
    Click { x: int, y: int },
}

fn handle(e: Event) -> string {
    match e {
        Event::Quit => "quit",
        Event::Key(c) => "key " + str(c),
        Event::Click { x, y } => fmt("clicked {},{}", x, y),
    }
}

fn size(n: int) -> string {
    match n {
        0 | 1 | 2 => "small",
        x if x > 100 => "big",
        _ => "medium",
    }
}

fn main() {
    println(handle(Event::Quit))
    println(handle(Event::Key('x')))
    println(handle(Event::Click { x: 3, y: 4 }))
    println(size(1))
    println(size(200))
}
