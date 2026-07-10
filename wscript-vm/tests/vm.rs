//! VM behavior through the public API: fault traces, unwinding, the call
//! depth limit and fuel metering.

use wscript_core::bytecode::CompiledUnit;
use wscript_core::registry::Registry;
use wscript_vm::Vm;

fn compile(src: &str) -> CompiledUnit {
    wscript_compiler::compile(src, &Registry::new())
        .unwrap_or_else(|d| panic!("test script failed to compile: {d:?}"))
        .unit
}

fn vm() -> Vm {
    Vm::new(&Registry::new())
}

#[test]
fn fault_carries_span_and_trace() {
    let unit = compile("fn main() -> int {\n    let z = 0\n    10 / z\n}");
    let err = vm().call_name(&unit, "main", vec![]).unwrap_err();
    assert_eq!(err.message, "division by zero");
    assert!(!err.trace.is_empty());
    assert_eq!(err.trace[0].function, "main");
    assert!(err.trace[0].span.is_some());
    assert_eq!(err.span, err.trace[0].span);
}

#[test]
fn fault_trace_is_innermost_first() {
    let unit = compile(
        "fn inner() -> int { 1 / 0 }\n\
         fn outer() -> int { inner() }\n\
         fn main() -> int { outer() }",
    );
    let err = vm().call_name(&unit, "main", vec![]).unwrap_err();
    let names: Vec<&str> = err.trace.iter().map(|f| f.function.as_str()).collect();
    assert_eq!(names, ["inner", "outer", "main"]);
}

#[test]
fn fault_unwinds_frames_vm_reusable() {
    let unit = compile("fn boom() -> int { 1 / 0 }\nfn ok() -> int { 42 }");
    let mut vm = vm();
    assert!(vm.call_name(&unit, "boom", vec![]).is_err());
    // The fault must have unwound its frames; the same VM keeps working.
    let v = vm.call_name(&unit, "ok", vec![]).unwrap();
    assert!(matches!(v, wscript_core::value::Value::Int(42)));
}

#[test]
fn call_depth_limit_faults() {
    let unit = compile("fn rec() -> int { rec() }\nfn main() -> int { rec() }");
    let err = vm().call_name(&unit, "main", vec![]).unwrap_err();
    assert!(err.message.contains("stack overflow"), "{}", err.message);
}

#[test]
fn call_depth_limit_zero_faults_immediately() {
    let unit = compile("fn main() -> int { 1 }");
    let mut vm = vm();
    vm.set_call_depth_limit(0);
    assert!(vm.call_name(&unit, "main", vec![]).is_err());
}

#[test]
fn fuel_exhaustion_faults_and_reads_zero() {
    let unit = compile("fn main() -> int {\n    let n = 0\n    while true { n = n + 1 }\n    n\n}");
    let mut vm = vm();
    vm.set_fuel(Some(100));
    let err = vm.call_name(&unit, "main", vec![]).unwrap_err();
    assert_eq!(err.message, "fuel exhausted");
    assert_eq!(vm.fuel(), Some(0));
}

#[test]
fn fuel_zero_edge_faults() {
    let unit = compile("fn main() -> int {\n    let n = 0\n    while true { n = n + 1 }\n    n\n}");
    let mut vm = vm();
    vm.set_fuel(Some(0));
    assert!(vm.call_name(&unit, "main", vec![]).is_err());
}

#[test]
fn fuel_persists_across_calls() {
    let unit = compile("fn f() -> int { 1 + 2 }");
    let mut vm = vm();
    vm.set_fuel(Some(1_000_000));
    vm.call_name(&unit, "f", vec![]).unwrap();
    let after_one = vm.fuel().unwrap();
    assert!(after_one < 1_000_000);
    vm.call_name(&unit, "f", vec![]).unwrap();
    let after_two = vm.fuel().unwrap();
    assert!(after_two < after_one);
}

#[test]
fn fuel_none_unmetered() {
    let unit =
        compile("fn main() -> int {\n    let n = 0\n    for i in 0..1000 { n = n + i }\n    n\n}");
    let mut vm = vm();
    let v = vm.call_name(&unit, "main", vec![]).unwrap();
    assert!(matches!(v, wscript_core::value::Value::Int(499500)));
    assert_eq!(vm.fuel(), None);
}

#[test]
fn call_name_unknown_fn_errs() {
    let unit = compile("fn main() -> int { 0 }");
    let err = vm().call_name(&unit, "nope", vec![]).unwrap_err();
    assert!(err.message.contains("no function named"), "{}", err.message);
}
