//! Interop tests (PRD §11): round-trip conversions for every
//! `FromValue`/`IntoValue` impl, host-signature misuse diagnostics,
//! aliasing-violation-returns-Err, ScriptFn handles, Shared values, and
//! cross-thread Context sharing (PRD §4.3).

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};

use wscript::{Context, Error, Module, Script, Shared, UnitExt, Value, Vm};

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
    m.const_("TITLE", "wscript");
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
            use wscript::{FromValue, IntoValue};
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

    let on_key: wscript::ScriptFn<(KeyEvent,), bool> = unit.fn_handle("on_key").unwrap();
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

    let add: wscript::ScriptFn<(i64, i64), i64> = unit.fn_handle("add").unwrap();
    assert_eq!(add.call(&mut vm, (20, 22)).unwrap(), 42);

    // Signature verified at lookup (PRD §6.4).
    let bad: Result<wscript::ScriptFn<(i64,), bool>, _> = unit.fn_handle("on_key");
    assert!(matches!(bad, Err(Error::Signature(_))));
    let missing: Result<wscript::ScriptFn<(), ()>, _> = unit.fn_handle("nope");
    assert!(matches!(missing, Err(Error::Signature(_))));
}

// ----------------------------------------------------------- stack trace

#[test]
fn runtime_fault_carries_full_stack_trace() {
    // A fault in `inner`, called from `outer`, called from `main`, must
    // produce a frame per call (innermost first), each with a source span.
    let ctx = Context::new();
    let unit = ctx
        .compile(
            "fn inner(xs: List[int]) -> int { xs[10] }\n\
             fn outer(xs: List[int]) -> int { inner(xs) }\n\
             fn main() -> int { let xs = [1, 2, 3]\n outer(xs) }",
        )
        .unwrap();
    let mut vm = Vm::new(&ctx);
    let err = match vm.call_values(&unit, "main", vec![]) {
        Err(Error::Runtime(e)) => e,
        other => panic!("expected a runtime fault, got {other:?}"),
    };

    assert!(
        err.message.contains("out of bounds"),
        "unexpected message: {}",
        err.message
    );
    let names: Vec<&str> = err.trace.iter().map(|f| f.function.as_str()).collect();
    assert_eq!(names, ["inner", "outer", "main"]);
    assert!(
        err.trace.iter().all(|f| f.span.is_some()),
        "every script frame should carry a source span: {:?}",
        err.trace
    );
    // `span` is a convenience mirror of the innermost frame's location.
    assert_eq!(err.span, err.trace[0].span);
    // The three frames sit on three distinct source lines.
    let spans: Vec<u32> = err
        .trace
        .iter()
        .filter_map(|f| f.span)
        .map(|s| s.lo)
        .collect();
    assert_eq!(spans.len(), 3);
    assert!(
        spans[0] != spans[1] && spans[1] != spans[2],
        "frames should point at distinct call sites: {spans:?}"
    );
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

#[test]
fn call_depth_limit_is_configurable() {
    let ctx = Context::new();
    let unit = ctx
        .compile(
            "fn rec(n: int) -> int { if n == 0 { 0 } else { rec(n - 1) } }\n\
             fn main(n: int) -> int { rec(n) }",
        )
        .unwrap();
    let mut vm = Vm::new(&ctx);
    assert_eq!(vm.call_depth_limit(), wscript::DEFAULT_CALL_DEPTH_LIMIT);

    vm.set_call_depth_limit(50);
    let ok: i64 = vm.call_unit(&unit, "main", (10_i64,)).unwrap();
    assert_eq!(ok, 0);
    let err = vm
        .call_unit::<_, i64>(&unit, "main", (100_i64,))
        .unwrap_err();
    assert!(err.to_string().contains("stack overflow"), "{err}");

    // Raising the limit lets the same call succeed.
    vm.set_call_depth_limit(200);
    let ok: i64 = vm.call_unit(&unit, "main", (100_i64,)).unwrap();
    assert_eq!(ok, 0);
}

// ------------------------------------------------------------------ fuel

#[test]
fn fuel_exhaustion_faults_with_trace() {
    let ctx = Context::new();
    let unit = ctx
        .compile(
            "fn spin() -> int { loop { } }\n\
             fn main() -> int { spin() }",
        )
        .unwrap();
    let mut vm = Vm::new(&ctx);
    vm.set_fuel(Some(1_000));
    let err = match vm.call_values(&unit, "main", vec![]) {
        Err(Error::Runtime(e)) => e,
        other => panic!("expected fuel exhaustion, got {other:?}"),
    };
    assert!(
        err.message.contains("fuel exhausted"),
        "unexpected message: {}",
        err.message
    );
    let names: Vec<&str> = err.trace.iter().map(|f| f.function.as_str()).collect();
    assert_eq!(names, ["spin", "main"]);
    assert!(err.span.is_some());
    assert_eq!(err.span, err.trace[0].span);
    assert_eq!(vm.fuel(), Some(0));
}

#[test]
fn fuel_exact_budget() {
    // Golden instruction count for a fixed script — doubles as a
    // bytecode-stability canary: if codegen changes, update K.
    //
    // This is the one case whose number is arrived at by hand, and it
    // stays that way: it reasons about an exact cost for one straight-line
    // block. Every other cost — every jump, call, return, host call,
    // re-entry and structural walk in the corpus — is recorded in
    // `tests/fuel.snap` by `wscript-cli/tests/fuel_snapshot.rs`.
    const K: u64 = 4;
    let ctx = Context::new();
    let unit = ctx.compile("fn main() -> int { 1 + 2 }").unwrap();
    let mut vm = Vm::new(&ctx);

    // Measure the cost against a generous tank and pin it.
    vm.set_fuel(Some(1_000));
    let ok: i64 = vm.call_unit(&unit, "main", ()).unwrap();
    assert_eq!(ok, 3);
    assert_eq!(vm.fuel(), Some(1_000 - K), "golden instruction count moved");

    // Exactly K fuel succeeds and leaves an empty tank...
    vm.set_fuel(Some(K));
    let ok: i64 = vm.call_unit(&unit, "main", ()).unwrap();
    assert_eq!(ok, 3);
    assert_eq!(vm.fuel(), Some(0));

    // ...and K - 1 faults.
    vm.set_fuel(Some(K - 1));
    let err = vm.call_unit::<_, i64>(&unit, "main", ()).unwrap_err();
    assert!(err.to_string().contains("fuel exhausted"), "{err}");
}

#[test]
fn fuel_zero_faults_before_any_call_completes() {
    let ctx = Context::new();
    let unit = ctx.compile("fn main() -> int { 0 }").unwrap();
    let mut vm = Vm::new(&ctx);
    vm.set_fuel(Some(0));
    let err = vm.call_unit::<_, i64>(&unit, "main", ()).unwrap_err();
    assert!(err.to_string().contains("fuel exhausted"), "{err}");
}

#[test]
fn fuel_default_is_unmetered() {
    let ctx = Context::new();
    let unit = ctx
        .compile(
            "fn main() -> int {\n\
                 let total = 0\n\
                 for i in 0..1000000 { total = total + 1 }\n\
                 total\n\
             }",
        )
        .unwrap();
    let mut vm = Vm::new(&ctx);
    assert_eq!(vm.fuel(), None);
    let n: i64 = vm.call_unit(&unit, "main", ()).unwrap();
    assert_eq!(n, 1_000_000);
    assert_eq!(vm.fuel(), None);
}

#[test]
fn fuel_depletes_across_calls() {
    // The tank belongs to the Vm, not to a call: host→script re-entry
    // (repeated ScriptFn calls) draws from the same budget until the
    // host sets it again.
    let ctx = Context::new();
    let unit = ctx
        .compile("fn add(a: int, b: int) -> int { a + b }\nfn main() {}")
        .unwrap();
    let mut vm = Vm::new(&ctx);
    let add: wscript::ScriptFn<(i64, i64), i64> = unit.fn_handle("add").unwrap();

    vm.set_fuel(Some(1_000));
    assert_eq!(add.call(&mut vm, (1, 2)).unwrap(), 3);
    let after_one = vm.fuel().unwrap();
    assert!(after_one < 1_000);
    assert_eq!(add.call(&mut vm, (3, 4)).unwrap(), 7);
    let after_two = vm.fuel().unwrap();
    assert_eq!(1_000 - after_one, after_one - after_two);

    // A tank sized for one call exhausts on the second.
    let cost = 1_000 - after_one;
    vm.set_fuel(Some(cost + cost - 1));
    assert_eq!(add.call(&mut vm, (5, 6)).unwrap(), 11);
    let err = add.call(&mut vm, (7, 8)).unwrap_err();
    assert!(err.to_string().contains("fuel exhausted"), "{err}");
}

#[test]
fn fuel_fault_is_not_script_observable() {
    // Exhaustion is a VM fault, not a script-level Err: a script cannot
    // catch its way past its budget with Result/match.
    let ctx = Context::new();
    let unit = ctx
        .compile(
            "fn spin() -> Result[int, string] { loop { } }\n\
             fn main() -> int {\n\
                 match spin() {\n\
                     Ok(n) => n,\n\
                     Err(e) => -1,\n\
                 }\n\
             }",
        )
        .unwrap();
    let mut vm = Vm::new(&ctx);
    vm.set_fuel(Some(1_000));
    let err = vm.call_unit::<_, i64>(&unit, "main", ()).unwrap_err();
    assert!(err.to_string().contains("fuel exhausted"), "{err}");
}

#[test]
fn vm_is_reusable_after_fuel_exhaustion() {
    let ctx = Context::new();
    let unit = ctx
        .compile(
            "fn spin() -> int { loop { } }\n\
             fn burn() -> int { spin() }\n\
             fn add(a: int, b: int) -> int { a + b }\n\
             fn main() {}",
        )
        .unwrap();
    let mut vm = Vm::new(&ctx);

    vm.set_fuel(Some(500));
    let err = vm.call_unit::<_, i64>(&unit, "burn", ()).unwrap_err();
    assert!(err.to_string().contains("fuel exhausted"), "{err}");

    // Refuelled, the same Vm works as after any other fault...
    vm.set_fuel(Some(1_000));
    let n: i64 = vm.call_unit(&unit, "add", (20_i64, 22_i64)).unwrap();
    assert_eq!(n, 42);

    // ...and unmetered again with None.
    vm.set_fuel(None);
    let n: i64 = vm.call_unit(&unit, "add", (1_i64, 2_i64)).unwrap();
    assert_eq!(n, 3);
}

#[test]
fn generic_fns_rejected_at_host_boundary() {
    let ctx = Context::new();
    let unit = ctx
        .compile(
            "fn identity[T](x: T) -> T { x }\n\
             fn identity_int(x: int) -> int { identity(x) }\n\
             fn main() {}",
        )
        .unwrap();

    // A typed handle on a generic fn is rejected with a targeted message.
    let handle: Result<wscript::ScriptFn<(i64,), i64>, _> = unit.fn_handle("identity");
    let Err(err) = handle else {
        panic!("generic fn_handle unexpectedly succeeded");
    };
    assert!(err.to_string().contains("is generic"), "{err}");

    // The monomorphic wrapper works.
    let mut vm = Vm::new(&ctx);
    let n: i64 = vm.call_unit(&unit, "identity_int", (7_i64,)).unwrap();
    assert_eq!(n, 7);
}

// ------------------------------------------------- script callbacks (§6.6)

fn callback_module() -> Module {
    use wscript::{Fault, HostCtx, ScriptClosure};
    let mut m = Module::new("cbs");
    // Applies the script closure twice; callback faults propagate as raw
    // VM faults with the callback's trace preserved.
    m.fn_(
        "apply_twice",
        |ctx: &mut dyn HostCtx, cb: ScriptClosure<(i64,), i64>, x: i64| -> Fault<i64> {
            Fault(cb.call(ctx, (x,)).and_then(|y| cb.call(ctx, (y,))))
        },
    );
    // Catches a callback fault and recovers — the VM must stay usable.
    m.fn_(
        "call_or",
        |ctx: &mut dyn HostCtx, cb: ScriptClosure<(i64,), i64>, dflt: i64| -> i64 {
            cb.call(ctx, (1,)).unwrap_or(dflt)
        },
    );
    // Calls the closure once — used to build host↔script ping-pong.
    m.fn_(
        "reenter",
        |ctx: &mut dyn HostCtx, cb: ScriptClosure<(i64,), i64>, x: i64| -> Fault<i64> {
            Fault(cb.call(ctx, (x,)))
        },
    );
    m
}

#[test]
fn callbacks_round_trip() {
    let ctx = Context::new().module(callback_module());
    let unit = ctx
        .compile(
            "use cbs\n\
             fn main() -> int { cbs::apply_twice(|x| x * 2, 5) }",
        )
        .unwrap();
    let mut vm = Vm::new(&ctx);
    let n: i64 = vm.call_unit(&unit, "main", ()).unwrap();
    assert_eq!(n, 20);
}

#[test]
fn callback_signature_checked_at_compile_time() {
    let ctx = Context::new().module(callback_module());
    let err = ctx
        .compile(
            "use cbs\n\
             fn main() -> int { cbs::apply_twice(|s: string| 1, 5) }",
        )
        .unwrap_err();
    let text = format!("{err:?}");
    assert!(text.contains("E022") || text.contains("type"), "{text}");
}

#[test]
fn callback_fault_carries_three_segment_trace() {
    let ctx = Context::new().module(callback_module());
    let unit = ctx
        .compile(
            "use cbs\n\
             fn main() -> int { cbs::apply_twice(|x| x / 0, 5) }",
        )
        .unwrap();
    let mut vm = Vm::new(&ctx);
    let err = vm.call_unit::<_, i64>(&unit, "main", ()).unwrap_err();
    let wscript::Error::Runtime(e) = err else {
        panic!("expected a runtime fault");
    };
    assert_eq!(e.message, "division by zero");
    let names: Vec<&str> = e.trace.iter().map(|f| f.function.as_str()).collect();
    // Callback frame first, then the host marker, then the outer frames.
    assert!(names[0].starts_with("<closure"), "{names:?}");
    assert!(names.contains(&"<host function>"), "{names:?}");
    assert_eq!(*names.last().unwrap(), "main", "{names:?}");
    // The innermost span points at the fault site inside the closure.
    assert!(e.trace[0].span.is_some());
}

#[test]
fn host_catches_callback_fault_and_vm_stays_usable() {
    let ctx = Context::new().module(callback_module());
    let unit = ctx
        .compile(
            "use cbs\n\
             fn boom() -> int { cbs::call_or(|x| x / 0, -7) }\n\
             fn fine() -> int { cbs::call_or(|x| x + 1, -7) }\n\
             fn main() {}",
        )
        .unwrap();
    let mut vm = Vm::new(&ctx);
    // The host recovered from the callback fault — no error escapes...
    let n: i64 = vm.call_unit(&unit, "boom", ()).unwrap();
    assert_eq!(n, -7);
    // ...and the same VM keeps working (frames were unwound).
    let n: i64 = vm.call_unit(&unit, "fine", ()).unwrap();
    assert_eq!(n, 2);
}

#[test]
fn fuel_meters_callback_instructions() {
    let ctx = Context::new().module(callback_module());
    let unit = ctx
        .compile(
            "use cbs\n\
             fn main() -> int {\n\
                 cbs::reenter(|x| { let n = 0; while true { n = n + 1 }; n }, 1)\n\
             }",
        )
        .unwrap();
    let mut vm = Vm::new(&ctx);
    vm.set_fuel(Some(10_000));
    let err = vm.call_unit::<_, i64>(&unit, "main", ()).unwrap_err();
    assert!(err.to_string().contains("fuel exhausted"), "{err}");
    assert_eq!(vm.fuel(), Some(0));
}

#[test]
fn host_script_ping_pong_faults_trappably() {
    let ctx = Context::new().module(callback_module());
    let unit = ctx
        .compile(
            "use cbs\n\
             fn ping(x: int) -> int { cbs::reenter(|y| ping(y), x) }\n\
             fn main() -> int { ping(1) }",
        )
        .unwrap();
    let mut vm = Vm::new(&ctx);
    let err = vm.call_unit::<_, i64>(&unit, "main", ()).unwrap_err();
    assert!(
        err.to_string().contains("re-entry too deep"),
        "expected trappable re-entry fault, got: {err}"
    );
    // Still usable afterwards.
    let unit2 = ctx.compile("fn ok() -> int { 3 }\nfn main() {}").unwrap();
    let n: i64 = vm.call_unit(&unit2, "ok", ()).unwrap();
    assert_eq!(n, 3);
}

// ------------------------------------------------ multi-file imports

/// In-memory [`wscript::SourceResolver`] for tests.
struct MemResolver(std::collections::HashMap<&'static str, &'static str>);

impl wscript::SourceResolver for MemResolver {
    fn resolve(
        &self,
        _from: &str,
        spec: wscript::ImportSpec,
    ) -> Result<wscript::ResolvedSource, String> {
        let key = match spec {
            wscript::ImportSpec::Name(n) => format!("{n}.wscript"),
            wscript::ImportSpec::Path(p) => p.trim_start_matches("./").to_string(),
        };
        match self.0.get(key.as_str()) {
            Some(src) => Ok(wscript::ResolvedSource {
                key: key.clone(),
                path: key,
                src: src.to_string(),
            }),
            None => Err("no such file".into()),
        }
    }
}

#[test]
fn multi_file_imports_compile_to_one_unit() {
    let resolver = MemResolver(
        [
            (
                "helpers.wscript",
                "fn double(x: int) -> int { x * 2 }\n\
                 fn best[T: Ord](a: T, b: T) -> T { if a > b { a } else { b } }",
            ),
            (
                "sub/geo.wscript",
                "struct Pt { x: int, y: int }\n\
                 fn origin() -> Pt { Pt { x: 3, y: 4 } }\n\
                 fn manhattan(p: Pt) -> int { p.x + p.y }",
            ),
        ]
        .into(),
    );
    let ctx = Context::new();
    let compiled = ctx
        .compile_entry(
            "main.wscript",
            "use helpers\n\
             use helpers::double\n\
             use \"./sub/geo.wscript\" as geo\n\
             fn main() -> int {\n\
                 helpers::double(2) + double(3) + helpers::best(1, 9) + geo::manhattan(geo::origin())\n\
             }",
            &resolver,
        )
        .unwrap_or_else(|f| panic!("compile failed: {:?}", f.diags));
    assert_eq!(compiled.unit.source_map.files.len(), 3);
    let mut vm = Vm::new(&ctx);
    let n: i64 = vm.call_unit(&compiled.unit, "main", ()).unwrap();
    assert_eq!(n, 4 + 6 + 9 + 7);
    // Imported files' fns are NOT exported.
    assert!(!compiled.unit.exports.contains_key("double"));
}

#[test]
fn multi_file_cycles_and_diamond_work() {
    let resolver = MemResolver(
        [
            (
                "a.wscript",
                "use b\nuse shared\n\
                 fn is_even(n: int) -> bool { if n == 0 { true } else { b::is_odd(n - 1) } }\n\
                 fn tag() -> string { shared::name() }",
            ),
            (
                "b.wscript",
                "use a\nuse shared\n\
                 fn is_odd(n: int) -> bool { if n == 0 { false } else { a::is_even(n - 1) } }",
            ),
            ("shared.wscript", "fn name() -> string { \"shared\" }"),
        ]
        .into(),
    );
    let ctx = Context::new();
    let compiled = ctx
        .compile_entry(
            "main.wscript",
            "use a\n\
             fn main() -> bool { a::is_even(10) }",
            &resolver,
        )
        .unwrap_or_else(|f| panic!("compile failed: {:?}", f.diags));
    // Diamond: `shared` loaded once despite two importers.
    assert_eq!(compiled.unit.source_map.files.len(), 4);
    let mut vm = Vm::new(&ctx);
    let even: bool = vm.call_unit(&compiled.unit, "main", ()).unwrap();
    assert!(even);
}

#[test]
fn multi_file_errors_point_at_the_right_file() {
    let resolver =
        MemResolver([("helpers.wscript", "fn broken() -> int { \"not an int\" }")].into());
    let ctx = Context::new();
    let failure = ctx
        .compile_entry(
            "main.wscript",
            "use helpers\nfn main() -> int { helpers::broken() }",
            &resolver,
        )
        .err()
        .expect("must fail");
    let type_err = failure
        .diags
        .iter()
        .find(|d| d.code == "E0220")
        .expect("type error expected");
    // The error's span lands in helpers.wscript per the source map.
    let (file, _) = failure
        .source_map
        .local(type_err.span.lo)
        .expect("span is in some file");
    assert_eq!(file.path, "helpers.wscript");
}

/// Derive validation runs once for the whole program, after the per-file
/// item passes — so its span has to come from where the type was
/// declared, not from whichever file was loaded last (#23).
#[test]
fn multi_file_derive_error_points_at_the_declaration() {
    const BAD: &str = "#[derive(Eq)]\nstruct Bad { f: fn(int) -> int }\nfn make() -> Bad { Bad { f: |x: int| -> int { x } } }";
    let resolver = MemResolver([("bad.wscript", BAD), ("last.wscript", "fn noop() {}")].into());
    let ctx = Context::new();
    let failure = ctx
        .compile_entry(
            "main.wscript",
            "use bad\nuse last\nfn main() { last::noop() }",
            &resolver,
        )
        .err()
        .expect("must fail");
    let derive_err = failure
        .diags
        .iter()
        .find(|d| d.code == "E0209")
        .expect("derive error expected");
    let (file, offset) = failure
        .source_map
        .local(derive_err.span.lo)
        .expect("span is in some file");
    assert_eq!(file.path, "bad.wscript");
    // The caret is on the type's name, not on the first byte of the file.
    assert_eq!(offset as usize, BAD.find("Bad").unwrap());
}

#[test]
fn missing_path_import_is_reported() {
    let resolver = MemResolver(Default::default());
    let ctx = Context::new();
    let failure = ctx
        .compile_entry(
            "main.wscript",
            "use \"./nope.wscript\"\nfn main() {}",
            &resolver,
        )
        .err()
        .expect("must fail");
    assert!(
        failure
            .diags
            .iter()
            .any(|d| d.message.contains("cannot load")),
        "{:?}",
        failure.diags
    );
}
