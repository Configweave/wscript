//! Declared parameter names (issue #22) surviving from registration into
//! the generated `.wscripti` interface — the text the LSP and
//! `wscript check` read, and the only place a host can say that `atan2`
//! takes `(y, x)` and not `(x, y)`.

use wscript::{Context, Module, Script};

#[derive(Script)]
#[script(opaque)]
struct Table {
    rows: Vec<String>,
}

fn geometry() -> Module {
    let mut m = Module::new("geometry");
    m.fn_named("atan2", ["y", "x"], |y: f64, x: f64| y.atan2(x));
    m.fn_("sqrt", |x: f64| x.sqrt());
    m.ty::<Table>()
        .method_named("row", ["index"], |t: &Table, index: i64| {
            t.rows.get(index as usize).cloned()
        })
        .method("len", |t: &Table| t.rows.len() as i64);
    m
}

fn interface() -> String {
    Context::new().module(geometry()).interface_text()
}

#[test]
fn declared_names_reach_the_interface() {
    let text = interface();
    assert!(
        text.contains("fn atan2(y: float, x: float) -> float"),
        "declared names should be emitted verbatim:\n{text}"
    );
    assert!(
        text.contains("fn row(self, index: int) -> Option[string]"),
        "method names too:\n{text}"
    );
}

#[test]
fn undeclared_parameters_stay_positional() {
    // Nothing was declared, so nothing is invented: `a0` is honest about
    // being a placeholder where a plausible name would read as fact.
    let text = interface();
    assert!(
        text.contains("fn sqrt(a0: float) -> float"),
        "undeclared parameters keep the positional placeholder:\n{text}"
    );
}

// The emit → load → emit round trip these names take is asserted in
// `wscript-cli/tests/wscripti_gen.rs`, over the whole standard library.

#[test]
#[should_panic(expected = "declares 3 parameter name(s) but takes 2")]
fn declaring_the_wrong_number_of_names_is_a_registration_error() {
    let mut m = Module::new("broken");
    m.fn_named("hypot", ["x", "y", "z"], |x: f64, y: f64| x.hypot(y));
    let _ = Context::new().module(m);
}

#[test]
#[should_panic(expected = "declares 0 parameter name(s) but takes 2")]
fn declaring_no_names_at_all_is_the_same_error() {
    // The mismatch that could otherwise pass for "this host declared
    // nothing" and silently degrade to positional placeholders.
    let mut m = Module::new("broken");
    m.fn_named("hypot", Vec::<&str>::new(), |x: f64, y: f64| x.hypot(y));
    let _ = Context::new().module(m);
}
