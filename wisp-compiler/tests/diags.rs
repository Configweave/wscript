//! Type checker positive/negative pairs (PRD §11): for each feature, a
//! snippet that must compile and a snippet that must fail with a specific
//! diagnostic code.

use wisp_core::registry::Registry;

fn codes(src: &str) -> Vec<&'static str> {
    match wisp_compiler::compile(src, &Registry::new()) {
        Ok(_) => vec![],
        Err(diags) => diags
            .iter()
            .filter(|d| d.severity == wisp_core::diag::Severity::Error)
            .map(|d| d.code)
            .collect(),
    }
}

fn ok(src: &str) {
    let result = codes(src);
    assert!(
        result.is_empty(),
        "expected clean compile, got {result:?}\n--- src ---\n{src}"
    );
}

fn fails_with(src: &str, code: &str) {
    let result = codes(src);
    assert!(
        result.contains(&code),
        "expected {code}, got {result:?}\n--- src ---\n{src}"
    );
}

#[test]
fn arithmetic_types() {
    ok("fn main() -> int { 1 + 2 * 3 }");
    fails_with("fn main() -> int { 1 + 2.0 }", "E0220");
    fails_with("fn main() -> int { \"a\" - \"b\" }", "E0234");
    ok("fn main() -> string { \"a\" + \"b\" }");
}

#[test]
fn annotations_required_on_fns() {
    fails_with("fn f(x) -> int { x }\nfn main() {}", "E0105");
    ok("fn f(x: int) -> int { x }\nfn main() { f(1); }");
}

#[test]
fn let_inference() {
    ok("fn main() -> int { let x = 5\n x }");
    ok("fn main() -> int { let x: int = 5\n x }");
    fails_with("fn main() { let x = [] }", "E0251");
    ok("fn main() -> int { let x = []\n x.push(1)\n x.len() }");
}

#[test]
fn condition_must_be_bool() {
    fails_with("fn main() { if 1 { } }", "E0227");
    fails_with("fn main() { while \"x\" { } }", "E0227");
}

#[test]
fn unknown_names() {
    fails_with("fn main() { missing() }", "E0230");
    fails_with("fn main() -> int { y }", "E0230");
    fails_with("fn main() -> Wat { }", "E0212");
}

#[test]
fn call_arity_and_types() {
    let prelude = "fn area(w: int, h: int) -> int { w * h }\n";
    ok(&format!("{prelude}fn main() -> int {{ area(2, 3) }}"));
    fails_with(&format!("{prelude}fn main() -> int {{ area(2) }}"), "E0238");
    fails_with(
        &format!("{prelude}fn main() -> int {{ area(\"oops\", 1) }}"),
        "E0220",
    );
}

#[test]
fn struct_literals() {
    let s = "struct P { x: int, y: int }\n";
    ok(&format!(
        "{s}fn main() -> int {{ let p = P {{ x: 1, y: 2 }}\n p.x }}"
    ));
    fails_with(&format!("{s}fn main() {{ P {{ x: 1 }} }}"), "E0247");
    fails_with(
        &format!("{s}fn main() {{ P {{ x: 1, y: 2, z: 3 }} }}"),
        "E0247",
    );
    fails_with(
        &format!("{s}fn main() {{ P {{ x: 1, y: \"s\" }} }}"),
        "E0220",
    );
    fails_with(
        &format!("{s}fn main() {{ let p = P {{ x: 1, y: 2 }}\n p.z }}"),
        "E0244",
    );
}

#[test]
fn enums_and_match() {
    let e = "enum E { A, B(int), C { v: bool } }\n";
    ok(&format!(
        "{e}fn main() -> int {{ match E::B(4) {{ E::A => 0, E::B(n) => n, E::C {{ v }} => if v {{ 1 }} else {{ 2 }} }} }}"
    ));
    // Missing variant → non-exhaustive.
    fails_with(
        &format!("{e}fn main() {{ match E::A {{ E::A => (), E::B(_) => () }} }}"),
        "E0260",
    );
    // Guards never count toward exhaustiveness.
    fails_with(
        &format!(
            "{e}fn main() {{ match E::A {{ E::A => (), E::B(_) => (), E::C {{ .. }} if true => () }} }}"
        ),
        "E0260",
    );
    // Unknown variant.
    fails_with(&format!("{e}fn main() {{ let x = E::Z }}"), "E0232");
    // Bool exhaustiveness.
    ok("fn main() -> int { match true { true => 1, false => 0 } }");
    fails_with("fn main() -> int { match true { true => 1 } }", "E0260");
    // Int needs a catch-all.
    fails_with("fn main() -> int { match 1 { 0 => 1, 1 => 2 } }", "E0260");
    ok("fn main() -> int { match 1 { 0 => 1, _ => 2 } }");
}

#[test]
fn nested_exhaustiveness() {
    // Exact at depth > 1 (better than the PRD's one-level guarantee).
    ok(
        "fn main() -> int { match Some(Some(1)) { Some(Some(n)) => n, Some(None) => 0, None => -1 } }",
    );
    fails_with(
        "fn main() -> int { match Some(Some(1)) { Some(Some(n)) => n, None => -1 } }",
        "E0260",
    );
}

#[test]
fn option_result_and_try() {
    ok("fn f() -> Option[int] { Some(1) }\nfn g() -> Option[int] { Some(f()? + 1) }\nfn main() {}");
    ok(
        "fn f() -> Result[int, string] { Ok(1) }\nfn g() -> Result[int, string] { Ok(f()? + 1) }\nfn main() {}",
    );
    // ? requires matching return type.
    fails_with(
        "fn f() -> Option[int] { Some(1) }\nfn g() -> int { f()? }\nfn main() {}",
        "E0249",
    );
    // Error types must line up.
    fails_with(
        "fn f() -> Result[int, string] { Ok(1) }\nfn g() -> Result[int, int] { Ok(f()?) }\nfn main() {}",
        "E0220",
    );
}

#[test]
fn containers() {
    ok("fn main() -> int { let m = #{ \"a\": 1 }\n m[\"a\"] }");
    fails_with("fn main() -> int { let xs = [1, 2]\n xs[\"a\"] }", "E0220");
    fails_with("fn main() { let m = #{ 1.5: 1 } }", "E0214");
    fails_with("fn main() { let xs = [1, \"a\"] }", "E0220");
    ok("fn main() -> List[int] { [1, 2].map(|x| x * 2) }");
    fails_with("fn main() { [|x: int| x].contains(|x: int| x) }", "E0242");
}

#[test]
fn strings_not_indexable() {
    fails_with("fn main() { \"abc\"[0] }", "E0245");
    ok("fn main() -> string { \"abc\".chars()\n \"abc\".slice(0, 1) }");
}

#[test]
fn let_else_must_diverge() {
    ok("fn main() -> int { let Some(x) = Some(1) else { return 0 }\n x }");
    fails_with(
        "fn main() -> int { let Some(x) = Some(1) else { }\n x }",
        "E0222",
    );
}

#[test]
fn or_pattern_bindings_rejected() {
    fails_with(
        "enum E { A(int), B(int) }\nfn main() -> int { match E::A(1) { E::A(n) | E::B(n) => n } }",
        "E0262",
    );
}

#[test]
fn closure_param_inference() {
    ok("fn apply(f: fn(int) -> int) -> int { f(1) }\nfn main() -> int { apply(|x| x + 1) }");
    fails_with("fn main() { let f = |x| x }", "E0250");
}

#[test]
fn no_user_generics() {
    fails_with(
        "struct Box { v: int }\nfn main() { let b: Box[int] = Box { v: 1 } }",
        "E0215",
    );
}

#[test]
fn assignment_targets() {
    ok("fn main() { let x = 1\n x = 2 }");
    fails_with("fn f() -> int { 1 }\nfn main() { f() = 2 }", "E0236");
}

#[test]
fn use_unknown_module() {
    fails_with("use nonexistent\nfn main() {}", "E0200");
}

#[test]
fn traits_and_impls() {
    let t = "trait Speak { fn speak(self) -> string }\nstruct D { x: int }\n";
    // Missing method in impl.
    fails_with(
        &format!("{t}impl Speak for D {{ }}\nfn main() {{}}"),
        "E0208",
    );
    // Wrong signature.
    fails_with(
        &format!("{t}impl Speak for D {{ fn speak(self) -> int {{ 1 }} }}\nfn main() {{}}"),
        "E0208",
    );
    // Coercion to dyn requires an impl.
    fails_with(
        &format!("{t}fn f(s: dyn Speak) {{}}\nfn main() {{ f(D {{ x: 1 }}) }}"),
        "E0223",
    );
    ok(&format!(
        "{t}impl Speak for D {{ fn speak(self) -> string {{ \"hi\" }} }}\n\
         fn f(s: dyn Speak) -> string {{ s.speak() }}\n\
         fn main() {{ f(D {{ x: 1 }}); }}"
    ));
    // Bare trait as a type needs dyn.
    fails_with(&format!("{t}fn f(s: Speak) {{}}\nfn main() {{}}"), "E0211");
}

#[test]
fn eq_requires_impl_or_derive() {
    let s = "struct P { x: int }\n";
    fails_with(
        &format!("{s}fn main() -> bool {{ P {{ x: 1 }} == P {{ x: 1 }} }}"),
        "E0235",
    );
    ok(&format!(
        "#[derive(Eq)]\n{s}fn main() -> bool {{ P {{ x: 1 }} == P {{ x: 1 }} }}"
    ));
}

#[test]
fn derives_validated() {
    fails_with(
        "#[derive(Hash)]\nstruct P { x: int }\nfn main() {}",
        "E0204",
    );
    fails_with("#[derive(Ord)]\nstruct P { x: int }\nfn main() {}", "E0204"); // Ord needs Eq
    fails_with(
        "#[derive(Eq)]\nstruct P { f: fn() -> int }\nfn main() {}",
        "E0209",
    );
}

#[test]
fn weak_refs() {
    ok(
        "struct N { v: int }\nfn main() { let n = N { v: 1 }\n let w: weak[N] = weak(n)\n w.upgrade(); }",
    );
    fails_with("fn main() { weak(5); }", "E0213");
    fails_with("fn main() { let w: weak[int] = weak(5) }", "E0213");
}

#[test]
fn no_impls_for_builtins_or_host() {
    fails_with("impl Option { fn f(self) {} }\nfn main() {}", "E0206");
}

#[test]
fn annotated_let_checks_init() {
    fails_with("fn main() { let x: string = 5 }", "E0220");
    ok("fn main() { let x: float = 1.5 }");
    // dyn coercion at annotated-let boundaries
    ok("trait T { fn f(self) -> int }\nstruct S { v: int }\n\
        impl T for S { fn f(self) -> int { self.v } }\n\
        fn main() { let d: dyn T = S { v: 1 }\n d.f(); }");
}
