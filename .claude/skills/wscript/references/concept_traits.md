# Traits

_Go-flavored interfaces with Rust syntax; static dispatch when the type is known, dyn Trait otherwise._

Traits are Go-flavored interfaces with Rust syntax. Dispatch is **static** when the
concrete type is known; `dyn Trait` selects **dynamic** (vtable) dispatch, with
implicit coercion at typed boundaries. There are no default method bodies and no
trait inheritance in v1.


```rust
trait Shape {
    fn area(self) -> float
}

impl Shape for Circle {
    fn area(self) -> float { 3.14159 * self.r * self.r }
}

fn describe(s: dyn Shape) -> string { str(s.area()) }   // vtable dispatch
let shapes: List[dyn Shape] = [Circle { r: 1.0 }, square]
```

> [!NOTE]
> **dyn at boundaries**
> A concrete type coerces to `dyn Trait` implicitly at a typed boundary (a `dyn Trait` parameter, or an element of a `List[dyn Trait]`).

## Examples

### Shapes — traits, dyn dispatch, operator overloading, derives

The M3 example. A `Shape` trait with two impls, dynamic dispatch through `List[dyn Shape]`, an `Add` overload, and `#[derive(Eq, Display, Clone)]`.

```rust
trait Shape {
    fn area(self) -> float
    fn describe(self) -> string
}

#[derive(Eq, Display, Clone)]
struct Rect { w: float, h: float }

#[derive(Eq, Display, Clone)]
struct Circle { r: float }

impl Shape for Rect {
    fn area(self) -> float { self.w * self.h }
    fn describe(self) -> string { fmt("rect {}x{}", self.w, self.h) }
}

impl Shape for Circle {
    fn area(self) -> float { 3.14159265 * self.r * self.r }
    fn describe(self) -> string { fmt("circle r={}", self.r) }
}

impl Add for Rect {
    fn add(self, other: Rect) -> Rect {
        Rect { w: self.w + other.w, h: self.h + other.h }
    }
}

fn total_area(shapes: List[dyn Shape]) -> float {
    shapes.fold(0.0, |acc, s| acc + s.area())
}

fn main() -> int {
    let shapes: List[dyn Shape] = [
        Rect { w: 2.0, h: 3.0 },
        Circle { r: 1.0 },
        Rect { w: 1.0, h: 1.0 },
    ]
    for s in shapes { println(s.describe()) }
    println(fmt("total: {}", total_area(shapes)))

    let a = Rect { w: 1.0, h: 2.0 }
    println(a + Rect { w: 2.0, h: 1.0 })
    println(a == a.clone())
    0
}
```

**Expected:** Describes each shape, prints the total area, an added Rect, and `true`.

## Related

- [Operator Overloading](../references/concept_operators.md)

- [Structs & Methods](../references/concept_structs.md)

- [Derives](../references/concept_derives.md)

[← Back to SKILL.md](../SKILL.md)
