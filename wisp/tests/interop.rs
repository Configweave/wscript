//! Interop tests (PRD §11): round-trip conversions for every
//! `FromValue`/`IntoValue` impl, host-signature misuse diagnostics,
//! aliasing-violation-returns-Err, ScriptFn handles, Shared values, and
//! cross-thread Context sharing (PRD §4.3).

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};

use wisp::{Context, Error, Module, Script, Shared, UnitExt, Value, Vm};

// ------------------------------------------------------------ fixtures

#[derive(Script, Debug, PartialEq, Clone)]
struct Point {
    x: i64,
    y: i64,
}

#[derive(Script, Debug, PartialEq, Clone)]
enum Event {
    Quit,
    Key(char),
    Click { x: i64, y: i64 },
}

#[derive(Script)]
#[script(opaque)]
struct Pane {
    title: String,
    splits: i64,
}

static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn term_module() -> Module {
    let mut m = Module::new("term");
    m.fn_("clear", || {
        LOG.lock().unwrap().push("clear".into());
    });
    m.fn_("print_at", |x: i64, y: i64, s: &str| {
        LOG.lock().unwrap().push(format!("print_at({x},{y},{s})"));
    });
    m.fn_("read_key", || -> Result<char, String> { Ok('q') });
    m.fn_("fail", || -> Result<i64, String> { Err("nope".into()) });
    m.const_("MAX_PANES", 16i64);
    m.const_("TITLE", "wisp");
    m
}

fn pane_module() -> Module {
    let mut m = Module::new("panes");
    m.ty::<Pane>()
        .method("title", |p: &Pane| p.title.clone())
        .method("split", |p: &mut Pane, vertical: bool| {
            p.splits += if vertical { 10 } else { 1 };
            p.splits
        })
        .method("rename", |p: &mut Pane, title: &str| {
            p.title = title.to_string();
        });
    m.fn_("active", || Pane {
        title: "main".into(),
        splits: 0,
    });
    m
}

// ------------------------------------------------- function registration

#[test]
fn host_fns_and_consts() {
    let ctx = Context::new().module(term_module());
    let unit = ctx
        .compile(
            "use term\n\
             fn main() -> int {\n\
                 term::clear()\n\
                 term::print_at(1, 2, \"hi\")\n\
                 match term::read_key() {\n\
                     Ok(c) => if c == 'q' { term::MAX_PANES } else { 0 },\n\
                     Err(e) => -1,\n\
                 }\n\
             }",
        )
        .unwrap();
    let mut vm = Vm::new(&ctx);
    let n: i64 = vm.call_unit(&unit, "main", ()).unwrap();
    assert_eq!(n, 16);
    let log = LOG.lock().unwrap();
    assert!(log.contains(&"clear".to_string()));
    assert!(log.contains(&"print_at(1,2,hi)".to_string()));
}

#[test]
fn host_err_maps_to_script_result() {
    let ctx = Context::new().module(term_module());
    let unit = ctx
        .compile(
            "use term\n\
             fn main() -> string {\n\
                 match term::fail() {\n\
                     Ok(n) => \"ok\",\n\
                     Err(e) => \"err: \" + e,\n\
                 }\n\
             }",
        )
        .unwrap();
    let mut vm = Vm::new(&ctx);
    let s: String = vm.call_unit(&unit, "main", ()).unwrap();
    assert_eq!(s, "err: nope");
}

#[test]
fn host_signature_misuse_is_compile_error() {
    let ctx = Context::new().module(term_module());
    // Wrong argument order/types (the PRD §6.1 example).
    let err = ctx
        .compile("use term\nfn main() { term::print_at(\"oops\", 1, 2) }")
        .unwrap_err();
    let Error::Compile(diags) = err else {
        panic!("expected compile error")
    };
    assert!(diags.iter().any(|d| d.code == "E0220"), "{diags:?}");
    // Wrong arity.
    let err = ctx
        .compile("use term\nfn main() { term::clear(1) }")
        .unwrap_err();
    let Error::Compile(diags) = err else {
        panic!("expected compile error")
    };
    assert!(diags.iter().any(|d| d.code == "E0238"), "{diags:?}");
}

// ------------------------------------------------------------ round-trips

#[test]
fn round_trip_all_conversions() {
    let ctx = Context::new().register_type::<Point>();
    let unit = ctx
        .compile("fn id(p: Point) -> Point { p }\nfn main() {}")
        .unwrap();
    let defs = &unit.defs;

    macro_rules! round_trip {
        ($ty:ty, $value:expr) => {{
            use wisp::{FromValue, IntoValue};
            let original: $ty = $value;
            let v = original.clone().into_value(defs).unwrap();
            let back = <$ty>::from_value(v, defs).unwrap();
            assert_eq!(original, back);
        }};
    }

    round_trip!(i64, -42);
    round_trip!(f64, 1.5);
    round_trip!(bool, true);
    round_trip!(char, 'λ');
    round_trip!(String, "héllo".to_string());
    round_trip!((), ());
    round_trip!(Vec<i64>, vec![1, 2, 3]);
    round_trip!(Vec<String>, vec!["a".to_string(), "b".to_string()]);
    round_trip!(Option<i64>, Some(5));
    round_trip!(Option<i64>, None);
    round_trip!(Result<i64, String>, Ok(1));
    round_trip!(Result<i64, String>, Err("bad".to_string()));
    round_trip!(HashMap<String, i64>, {
        let mut m = HashMap::new();
        m.insert("a".to_string(), 1);
        m
    });
    round_trip!(HashMap<i64, bool>, {
        let mut m = HashMap::new();
        m.insert(3, true);
        m
    });
    round_trip!(Point, Point { x: 1, y: -2 });
    round_trip!(Vec<Point>, vec![Point { x: 0, y: 0 }]);
    round_trip!(Option<Point>, Some(Point { x: 7, y: 8 }));
}

#[test]
fn derived_enum_round_trip_and_match() {
    let ctx = Context::new().register_type::<Event>();
    let unit = ctx
        .compile(
            "fn describe(e: Event) -> string {\n\
                 match e {\n\
                     Event::Quit => \"quit\",\n\
                     Event::Key(c) => \"key \" + str(c),\n\
                     Event::Click { x, y } => fmt(\"click {},{}\", x, y),\n\
                 }\n\
             }\n\
             fn make() -> Event { Event::Click { x: 3, y: 4 } }\n\
             fn main() {}",
        )
        .unwrap();
    let mut vm = Vm::new(&ctx);
    // Host → script (all three variant kinds).
    let s: String = vm.call_unit(&unit, "describe", (Event::Quit,)).unwrap();
    assert_eq!(s, "quit");
    let s: String = vm.call_unit(&unit, "describe", (Event::Key('x'),)).unwrap();
    assert_eq!(s, "key x");
    let s: String = vm
        .call_unit(&unit, "describe", (Event::Click { x: 1, y: 2 },))
        .unwrap();
    assert_eq!(s, "click 1,2");
    // Script → host.
    let e: Event = vm.call_unit(&unit, "make", ()).unwrap();
    assert_eq!(e, Event::Click { x: 3, y: 4 });
}

#[test]
fn data_types_cross_by_value() {
    let ctx = Context::new().register_type::<Point>();
    let unit = ctx
        .compile("fn bump(p: Point) -> Point { p.x = p.x + 1\n p }\nfn main() {}")
        .unwrap();
    let mut vm = Vm::new(&ctx);
    let original = Point { x: 1, y: 2 };
    let bumped: Point = vm.call_unit(&unit, "bump", (original.clone(),)).unwrap();
    assert_eq!(bumped, Point { x: 2, y: 2 });
    // The host's copy is unaffected: data converts by value (PRD §6.2).
    assert_eq!(original, Point { x: 1, y: 2 });
}

// ------------------------------------------------------- opaque handles

#[test]
fn opaque_methods_and_mutation() {
    let ctx = Context::new().module(pane_module());
    let unit = ctx
        .compile(
            "use panes\n\
             fn main() -> string {\n\
                 let p = panes::active()\n\
                 p.split(true)\n\
                 p.split(false)\n\
                 p.rename(\"renamed\")\n\
                 p.title()\n\
             }",
        )
        .unwrap();
    let mut vm = Vm::new(&ctx);
    let title: String = vm.call_unit(&unit, "main", ()).unwrap();
    assert_eq!(title, "renamed");
}

#[test]
fn opaque_fields_inaccessible_from_script() {
    let ctx = Context::new().module(pane_module());
    let err = ctx
        .compile("use panes\nfn main() { let p = panes::active()\n p.title = \"x\" }")
        .unwrap_err();
    let Error::Compile(diags) = err else {
        panic!("expected compile error")
    };
    assert!(diags.iter().any(|d| d.code == "E0244"), "{diags:?}");
    // Nor constructible.
    let err = ctx
        .compile("use panes\nfn main() { Pane { } }")
        .unwrap_err();
    let Error::Compile(diags) = err else {
        panic!("expected compile error")
    };
    assert!(diags.iter().any(|d| d.code == "E0246"), "{diags:?}");
}

#[test]
fn aliasing_violation_returns_err_not_panic() {
    // A method that re-enters the same opaque value while it is mutably
    // borrowed must produce Err (PRD §6.5).
    #[derive(Script)]
    #[script(opaque)]
    struct Spinny {
        n: i64,
    }

    let mut m = Module::new("spin");
    m.ty::<Spinny>()
        .method("get", |s: &Spinny| s.n)
        .method("set", |s: &mut Spinny, v: i64| {
            s.n = v;
        });
    m.fn_("make", || Spinny { n: 1 });

    let ctx = Context::new().module(m);
    let unit = ctx
        .compile(
            "use spin\n\
             fn make() -> Spinny { spin::make() }\n\
             fn poke(s: Spinny) -> int { s.set(5)\n s.get() }\n\
             fn main() {}",
        )
        .unwrap();
    let mut vm = Vm::new(&ctx);
    let handle = vm.call_values(&unit, "make", vec![]).unwrap();
    let n = vm.call_values(&unit, "poke", vec![handle.clone()]).unwrap();
    assert!(matches!(n, Value::Int(5)));

    // Hold a host borrow, then let the script call a &mut method on the
    // same live value: trappable Err, not a panic.
    let shared: Shared<Spinny> = Shared::from_value(handle.clone(), &unit);
    let guard = shared.borrow().unwrap();
    assert_eq!(guard.n, 5);
    let result = vm.call_values(&unit, "poke", vec![handle]);
    match result {
        Err(Error::Runtime(e)) => assert!(
            e.message.contains("aliasing violation"),
            "unexpected message: {}",
            e.message
        ),
        other => panic!("expected aliasing Err, got {other:?}"),
    }
    drop(guard);

    // Borrow conflicts via Shared directly.
    let b1 = shared.borrow_mut().unwrap();
    assert!(shared.borrow().is_err(), "second borrow must Err");
    drop(b1);
    assert!(shared.borrow().is_ok());
}

// ------------------------------------------------------------- ScriptFn

#[test]
fn script_fn_typed_handles() {
    #[derive(Script, Clone)]
    struct KeyEvent {
        code: char,
        ctrl: bool,
    }

    let ctx = Context::new().register_type::<KeyEvent>();
    let unit = ctx
        .compile(
            "fn on_key(e: KeyEvent) -> bool { e.ctrl && e.code == 'q' }\n\
             fn add(a: int, b: int) -> int { a + b }\n\
             fn main() {}",
        )
        .unwrap();
    let mut vm = Vm::new(&ctx);

    let on_key: wisp::ScriptFn<(KeyEvent,), bool> = unit.fn_handle("on_key").unwrap();
    let quit = on_key
        .call(
            &mut vm,
            (KeyEvent {
                code: 'q',
                ctrl: true,
            },),
        )
        .unwrap();
    assert!(quit);

    let add: wisp::ScriptFn<(i64, i64), i64> = unit.fn_handle("add").unwrap();
    assert_eq!(add.call(&mut vm, (20, 22)).unwrap(), 42);

    // Signature verified at lookup (PRD §6.4).
    let bad: Result<wisp::ScriptFn<(i64,), bool>, _> = unit.fn_handle("on_key");
    assert!(matches!(bad, Err(Error::Signature(_))));
    let missing: Result<wisp::ScriptFn<(), ()>, _> = unit.fn_handle("nope");
    assert!(matches!(missing, Err(Error::Signature(_))));
}

// --------------------------------------------------------------- Shared

#[test]
fn shared_data_value_mutation_visible_both_ways() {
    let ctx = Context::new().register_type::<Point>();
    let unit = ctx
        .compile(
            "fn make() -> Point { Point { x: 1, y: 2 } }\n\
             fn read_x(p: Point) -> int { p.x }\n\
             fn main() {}",
        )
        .unwrap();
    let mut vm = Vm::new(&ctx);
    let raw = vm.call_values(&unit, "make", vec![]).unwrap();
    let shared: Shared<Point> = Shared::from_value(raw.clone(), &unit);

    // Host observes script state.
    assert_eq!(shared.get().unwrap(), Point { x: 1, y: 2 });
    // Host mutates; script observes through the same live value.
    shared.set(Point { x: 99, y: 2 }).unwrap();
    let x: i64 = vm
        .call_unit(&unit, "read_x", (shared.get().unwrap(),))
        .unwrap();
    assert_eq!(x, 99);
    // And through the raw aliased value:
    let x = vm.call_values(&unit, "read_x", vec![raw]).unwrap();
    assert!(matches!(x, Value::Int(99)));
}

// ------------------------------------------------------ thread sharing

#[test]
fn context_and_unit_shared_across_threads() {
    // PRD §4.3 / §13: one compilation, many per-thread VMs.
    static COUNTER: AtomicI64 = AtomicI64::new(0);
    let mut m = Module::new("counter");
    m.fn_("bump", || COUNTER.fetch_add(1, Ordering::SeqCst) + 1);

    let ctx = Context::new().module(m);
    let unit = ctx
        .compile("use counter\nfn main() -> int { counter::bump() }")
        .unwrap();

    let handles: Vec<_> = (0..4)
        .map(|_| {
            let ctx = ctx.clone();
            let unit = unit.clone();
            std::thread::spawn(move || {
                let mut vm = Vm::new(&ctx);
                let n: i64 = vm.call_unit(&unit, "main", ()).unwrap();
                n
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(COUNTER.load(Ordering::SeqCst), 4);
}
