// Traits: static dispatch, dyn Trait vtable dispatch, implicit coercion.
// expect: woof
// expect: meow
// expect: woof
// expect: meow meow
// expect: 2

trait Speak {
    fn speak(self) -> string
    fn repeat(self, n: int) -> string
}

struct Dog { name: string }
struct Cat { lives: int }

impl Speak for Dog {
    fn speak(self) -> string { "woof" }
    fn repeat(self, n: int) -> string {
        let parts = []
        for i in 0..n { parts.push(self.speak()) }
        parts.join(" ")
    }
}

impl Speak for Cat {
    fn speak(self) -> string { "meow" }
    fn repeat(self, n: int) -> string {
        let parts = []
        for i in 0..n { parts.push(self.speak()) }
        parts.join(" ")
    }
}

fn announce(s: dyn Speak) -> string { s.speak() }

fn main() {
    let d = Dog { name: "rex" }
    let c = Cat { lives: 9 }
    // static dispatch
    println(d.speak())
    println(c.speak())
    // dyn dispatch with implicit coercion
    println(announce(d))
    let animals: List[dyn Speak] = [d, c]
    println(animals[1].repeat(2))
    println(animals.len())
}
