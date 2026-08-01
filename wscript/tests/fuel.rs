//! Fuel accounting at the embedding boundary (PRD §5.2): what a control
//! transfer costs, and what a host sees when the tank runs dry.
//!
//! Fuel is charged at control transfers — a straight-line block at a time,
//! paid for by the jump, call or return that ends it. Every transfer arm
//! the dispatch loop has therefore gets a *family* of scripts here whose
//! only variable is how many times that transfer happens, and its cost is
//! asserted to be exactly affine in that count: `base + k * slope`.
//!
//! The pinned `slope` is a golden per-transfer number, and like any golden
//! number it moves when codegen changes — the failure message says so and
//! reports the observed fit. The real quarry is a *non-affine* cost: an arm
//! that forgets to charge flattens its slope (the transfer, and the block
//! it ends, become free), and one that forgets to restart the block bills
//! the same instructions twice or underflows into "fuel exhausted". Either
//! way the family stops fitting.
//!
//! `tests/fuel.snap` (`wscript-cli/tests/fuel_snapshot.rs`) records what
//! the script corpus costs end to end; this suite isolates one transfer at
//! a time.

use wscript::{Context, Error, Fault, HostCtx, Module, ScriptClosure, UnitExt, Vm};

/// Bigger than any case here needs, so nothing measured is a truncated
/// run. Fuel is charged in dispatched instructions, so the size of the
/// tank does not affect what a run costs.
const TANK: u64 = 1_000_000;

/// How many repetitions each family is measured at. Four points, so a cost
/// that is merely *monotonic* in the repetition count — an arm charging
/// something other than the block it ended — still fails to fit.
const REPS: [usize; 4] = [1, 2, 3, 4];

/// Host functions the families call: a plain host call, and one that calls
/// back into the VM (the re-entry path, where the dispatch loop hands its
/// tank to a nested run and takes back what is left).
fn hosts() -> Module {
    let mut m = Module::new("h");
    m.fn_("id", |x: i64| x);
    m.fn_(
        "apply",
        |ctx: &mut dyn HostCtx, cb: ScriptClosure<(i64,), i64>, x: i64| -> Fault<i64> {
            Fault(cb.call(ctx, (x,)))
        },
    );
    // Catches a callback fault and recovers, so the script runs on after
    // one: the tank has to have come back from the *failed* nested run.
    m.fn_(
        "or",
        |ctx: &mut dyn HostCtx, cb: ScriptClosure<(i64,), i64>, dflt: i64| -> i64 {
            cb.call(ctx, (1,)).unwrap_or(dflt)
        },
    );
    m
}

fn host_context() -> Context {
    Context::new().module(hosts())
}

/// Run `source`'s `main` against a full tank and report what it drew.
fn cost(ctx: &Context, source: &str) -> u64 {
    let unit = ctx
        .compile(source)
        .unwrap_or_else(|e| panic!("case does not compile: {e}\n--- source ---\n{source}"));
    let mut vm = Vm::new(ctx);
    vm.set_fuel(Some(TANK));
    vm.call_values(&unit, "main", vec![])
        .unwrap_or_else(|e| panic!("case faulted: {e}\n--- source ---\n{source}"));
    TANK - vm.fuel().expect("the VM was built metered")
}

/// One kind of control transfer, and a family of scripts whose only
/// variable is how many times the dispatch loop takes it.
struct Case {
    /// The dispatch arm, in the VM's own words.
    what: &'static str,
    /// The family: `k` repetitions of the transfer.
    source: fn(usize) -> String,
    /// Fuel a hypothetical `k = 0` member would cost — the fit's intercept,
    /// pinned so a change in the surrounding scaffolding is visible too.
    base: u64,
    /// Fuel one more repetition costs: the transfer itself plus the
    /// straight-line block it ends.
    slope: u64,
}

const TRANSFERS: &[Case] = &[
    Case {
        what: "a backward jump (a loop iteration)",
        source: backward_jump,
        base: 9,
        slope: 8,
    },
    Case {
        what: "a taken forward jump (an `if` whose body is skipped)",
        source: forward_jump,
        base: 5,
        slope: 2,
    },
    Case {
        what: "a script call and the return from it",
        source: script_call,
        base: 3,
        slope: 5,
    },
    Case {
        what: "a closure call (`CallValue`) and the return from it",
        source: closure_call,
        base: 4,
        slope: 6,
    },
    Case {
        what: "a dynamic-dispatch call (`CallVirtual`) and its return",
        source: virtual_call,
        base: 8,
        slope: 6,
    },
    Case {
        what: "a host call",
        source: host_call,
        base: 3,
        slope: 3,
    },
    Case {
        what: "a builtin call",
        source: builtin_call,
        base: 5,
        slope: 4,
    },
    Case {
        what: "a call returning unit (`RetUnit`), which a callee also \
               falls off the end of its code into",
        source: unit_return,
        base: 3,
        slope: 3,
    },
    Case {
        what: "an instruction inside a host→script re-entry (the callback \
               body draws from the caller's tank)",
        source: re_entry,
        base: 5,
        slope: 2,
    },
    Case {
        what: "a loop iteration inside a re-entrant callback that then \
               faults and is caught by the host (the tank comes back from \
               a nested run that ended in Err)",
        source: re_entry_caught,
        base: 16,
        slope: 8,
    },
];

/// `k` loop iterations: each one runs the body and takes the backward jump
/// that ends it.
fn backward_jump(k: usize) -> String {
    format!("fn main() -> int {{\n    let t = 0\n    for i in 0..{k} {{ t = t + 1 }}\n    t\n}}")
}

/// `k` `if`s over a condition that is false at runtime (so nothing is
/// constant-folded): the body is never dispatched, and all the repetition
/// costs is the taken jump over it.
fn forward_jump(k: usize) -> String {
    let mut body = String::from("fn main() -> int {\n    let t = 0\n    let never = t < 0\n");
    for _ in 0..k {
        body.push_str("    if never { t = t + 1 }\n");
    }
    body.push_str("    t\n}");
    body
}

/// `k` calls to a script function, each with its own `Ret`.
fn script_call(k: usize) -> String {
    let mut body = String::from("fn one() -> int { 1 }\nfn main() -> int {\n    let t = 0\n");
    for _ in 0..k {
        body.push_str("    t = t + one()\n");
    }
    body.push_str("    t\n}");
    body
}

/// `k` calls through a function value.
fn closure_call(k: usize) -> String {
    let mut body =
        String::from("fn main() -> int {\n    let f = |x: int| -> int { x + 1 }\n    let t = 0\n");
    for _ in 0..k {
        body.push_str("    t = f(t)\n");
    }
    body.push_str("    t\n}");
    body
}

/// `k` method calls on a `dyn` receiver — dispatched through the vtable.
fn virtual_call(k: usize) -> String {
    let mut body = String::from(
        "trait Beep { fn beep(self) -> int }\n\
         struct B { v: int }\n\
         impl Beep for B { fn beep(self) -> int { self.v } }\n\
         fn many(s: dyn Beep) -> int {\n    let t = 0\n",
    );
    for _ in 0..k {
        body.push_str("    t = t + s.beep()\n");
    }
    body.push_str("    t\n}\nfn main() -> int { many(B { v: 1 }) }");
    body
}

/// `k` host calls — charged 1 for the dispatch; what the host function
/// does internally is not metered.
fn host_call(k: usize) -> String {
    let mut body = String::from("use h\nfn main() -> int {\n    let t = 0\n");
    for _ in 0..k {
        body.push_str("    t = h::id(t)\n");
    }
    body.push_str("    t\n}");
    body
}

/// `k` builtin calls. Like a host call, the builtin arm leaves the
/// dispatch loop for native code that may re-enter it, so it ends its
/// block for the same reason.
fn builtin_call(k: usize) -> String {
    let mut body = String::from("fn main() -> int {\n    let xs = [1]\n    let t = 0\n");
    for _ in 0..k {
        body.push_str("    t = t + xs.len()\n");
    }
    body.push_str("    t\n}");
    body
}

/// `k` extra instructions in the body of a callback a *host function*
/// invokes. They are dispatched by a nested `execute`, which draws from
/// `Vm::fuel` — so they only show up in the bill if the outer loop handed
/// its tank over and took back what was left.
fn re_entry(k: usize) -> String {
    let adds = "+ 0 ".repeat(k);
    format!("use h\nfn main() -> int {{ h::apply(|x| x {adds}, 5) }}")
}

/// `k` calls to a function with no return value: the callee falls off the
/// end of its code, which the loop dispatches as `RetUnit`.
fn unit_return(k: usize) -> String {
    let mut body = String::from("fn nop() {}\nfn main() -> int {\n    let t = 0\n");
    for _ in 0..k {
        body.push_str("    nop()\n");
    }
    body.push_str("    t\n}");
    body
}

/// A callback that spends `k` loop iterations and *then* faults, with the
/// host catching the fault and returning a default. What it spent before
/// faulting has to reach the outer loop's bill: the nested run drew from
/// `Vm::fuel`, and the outer loop only sees that if it takes the tank back
/// on the error path as well as the success one.
fn re_entry_caught(k: usize) -> String {
    format!(
        "use h\n\
         fn burn(n: int) -> int {{\n    let t = 0\n    for i in 0..n {{ t = t + 1 }}\n    t\n}}\n\
         fn main() -> int {{ h::or(|x| burn({k}) / 0, -7) }}"
    )
}

/// The fit of a family's measurements, reported when it fails so the
/// pinned numbers can be updated (or disbelieved) without re-deriving
/// them by hand.
fn observed(measured: &[(usize, u64)]) -> String {
    measured
        .iter()
        .map(|(k, fuel)| format!("k={k}: {fuel}"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[test]
fn every_transfer_charges_its_block() {
    let ctx = host_context();
    for transfer in TRANSFERS {
        let measured: Vec<(usize, u64)> = REPS
            .iter()
            .map(|&k| (k, cost(&ctx, &(transfer.source)(k))))
            .collect();
        for &(k, fuel) in &measured {
            let expected = transfer.base + k as u64 * transfer.slope;
            assert_eq!(
                fuel,
                expected,
                "{} cost {fuel} fuel at {k} repetition(s), not the pinned \
                 {} + {k} × {} = {expected}.\n  observed: {}\n\
                 A slope that collapsed means the arm stopped charging; one \
                 that grew means it charged a block twice or failed to \
                 restart it. If codegen changed instead, re-derive the fit.",
                transfer.what,
                transfer.base,
                transfer.slope,
                observed(&measured),
            );
        }
    }
}

#[test]
fn every_transfer_faults_one_fuel_short() {
    // Exhaustion surfaces at the charge point, so a tank sized to the
    // measured cost is exactly enough and one less is not — for every
    // transfer, not just for the straight-line block that used to be the
    // only case this was checked on.
    let ctx = host_context();
    for transfer in TRANSFERS {
        let source = (transfer.source)(REPS[REPS.len() - 1]);
        let exact = cost(&ctx, &source);
        let unit = ctx.compile(&source).unwrap();

        let mut vm = Vm::new(&ctx);
        vm.set_fuel(Some(exact));
        vm.call_values(&unit, "main", vec![]).unwrap_or_else(|e| {
            panic!(
                "{} did not fit in its own measured cost of {exact}: {e}",
                transfer.what
            )
        });
        assert_eq!(vm.fuel(), Some(0), "{} left fuel behind", transfer.what);

        let mut vm = Vm::new(&ctx);
        vm.set_fuel(Some(exact - 1));
        let err = vm
            .call_values(&unit, "main", vec![])
            .expect_err(transfer.what);
        assert!(
            err.to_string().contains("fuel exhausted"),
            "{} faulted with something other than exhaustion: {err}",
            transfer.what
        );
    }
}

// ---------------------------------------------------------- tank behaviour

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
