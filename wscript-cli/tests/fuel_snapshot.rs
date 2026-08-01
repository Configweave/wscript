//! Fuel cost snapshot: what every script in the corpus actually costs to
//! run, recorded so a refactor of fuel accounting can be proven not to
//! change it.
//!
//! Fuel is observable behaviour. A host that sets a per-tick budget sees a
//! changed cost as a script that suddenly faults, or one that suddenly does
//! not — so "the numbers moved" is a behavioural diff, not an
//! implementation detail. The only signal before this suite was
//! `fuel_exact_budget`'s `const K = 4` over `fn main() -> int { 1 + 2 }`:
//! one straight-line block, with no jump, no call, no return, no host call
//! and no structural operation. Those are precisely the control-transfer
//! points fuel is charged at, and precisely what nothing measured.
//!
//! Each case runs in-process against a large tank; its cost is
//! `TANK - vm.fuel()`. The committed table is `tests/fuel.snap` and
//! `just fuel-regen` rewrites it — the `std.wscripti` / diagnostic-snapshot
//! pattern this workspace already uses: a committed file, an environment
//! variable to regenerate, a test that fails when the committed copy is
//! stale, and no third-party dev-dependency.
//!
//! This suite *records* costs; it does not judge them. A cost that looks
//! wrong is a separate ticket — folding a fix in here would destroy the
//! baseline the accounting refactor needs.

use std::path::{Path, PathBuf};

use wscript::{
    Context, Error, Fault, HostCtx, Module, PrintHook, ScriptClosure, Session, Vm, VmConfig,
};
use wscript_cli::manifest::{Mode, project_for};
use wscript_core::bytecode::{Builtin, CallTarget, CompiledUnit, Instr};

/// Bigger than any corpus script needs, so nothing in the table is a
/// truncated run. Fuel is charged in dispatched instructions (PRD §5.2),
/// a straight-line block at a time, so the size of the tank does not
/// affect what a run costs.
const TANK: u64 = 1_000_000_000;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn snapshot_path() -> PathBuf {
    workspace_root().join("tests/fuel.snap")
}

/// Every corpus script, sorted, so the table's order is stable across
/// filesystems. Globbed rather than listed for the same reason `build.rs`
/// globs: a new script is covered by adding the file.
fn corpus() -> Vec<PathBuf> {
    let dir = workspace_root().join("tests/scripts");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "wscript"))
        .collect();
    assert!(!paths.is_empty(), "no scripts under {}", dir.display());
    paths.sort();
    paths
}

/// Script output goes nowhere: the corpus asserts on it elsewhere
/// (`scripts.rs`), and here it would only pollute the test log.
fn null_print_hook() -> PrintHook {
    Box::new(|_, _| {})
}

/// How a case's run ended. The table writes the tag; the header explains
/// it — both from [`Outcome::described`], so a new outcome cannot reach
/// the table without reaching the legend.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Outcome {
    Ok,
    Exit,
    Fault,
    HostError,
    CompileError,
}

impl Outcome {
    /// Every outcome, in the order the legend lists them.
    const ALL: &'static [Outcome] = &[
        Outcome::Ok,
        Outcome::Exit,
        Outcome::Fault,
        Outcome::HostError,
        Outcome::CompileError,
    ];

    /// The tag written in the table, and the lines the header explains it
    /// with. One match, so the two cannot drift apart.
    fn described(self) -> (&'static str, &'static [&'static str]) {
        match self {
            Outcome::Ok => ("ok", &["`main` returned; the cost is exact"]),
            Outcome::Exit => ("exit", &["`process::exit`; the cost is exact"]),
            Outcome::Fault => (
                "fault",
                &[
                    "a runtime fault — a fault abandons its straight-line",
                    "block uncharged, so this is the fuel charged, not the",
                    "fuel a completed run would have cost",
                ],
            ),
            Outcome::HostError => (
                "host-error",
                &[
                    "the call failed at the host boundary (conversion or",
                    "signature) rather than in the script",
                ],
            ),
            Outcome::CompileError => (
                "compile-error",
                &[
                    "never ran (an `_err` fixture); its snapshot is the",
                    "diagnostic one",
                ],
            ),
        }
    }

    fn tag(self) -> &'static str {
        self.described().0
    }
}

/// What one case cost, and how its run ended.
struct Cost {
    /// Fuel charged, or `None` when the case never ran.
    fuel: Option<u64>,
    outcome: Outcome,
}

impl Cost {
    fn line(&self, name: &str) -> String {
        match self.fuel {
            Some(fuel) => format!("{name} {fuel} {}\n", self.outcome.tag()),
            None => format!("{name} - {}\n", self.outcome.tag()),
        }
    }
}

/// Run `unit`'s `main` on a metered VM built from `ctx` and report what it
/// drew from the tank.
fn measure(ctx: &Context, unit: &CompiledUnit) -> Cost {
    let mut vm = Vm::with_config(
        ctx,
        VmConfig {
            fuel: Some(TANK),
            out: null_print_hook(),
            ..VmConfig::default()
        },
    );
    let outcome = match vm.call_values(unit, "main", vec![]) {
        Ok(_) => Outcome::Ok,
        // `process::exit` — a requested exit, not a fault.
        Err(Error::Runtime(e)) if e.exit_code.is_some() => Outcome::Exit,
        Err(Error::Runtime(_)) => Outcome::Fault,
        Err(_) => Outcome::HostError,
    };
    let remaining = vm.fuel().expect("the VM was built metered");
    Cost {
        fuel: Some(TANK - remaining),
        outcome,
    }
}

/// Compile one corpus script through the same session the CLI would build
/// for it (`project_for` + `Mode::Run`), so what is measured is the cost
/// of `wscript run <script>`.
///
/// `None` for the `_err` fixtures: they are compile-error scripts, so they
/// have a cost of nothing and their real snapshot is the diagnostic one.
fn compile_corpus_script(path: &Path) -> Option<(Session, CompiledUnit)> {
    let source = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let session = project_for(
        path,
        Mode::Run {
            script_args: Vec::new(),
        },
    )
    .session;
    let compiled = session.compile(&path.to_string_lossy(), &source).ok()?;
    Some((session, compiled.unit))
}

fn measure_script(path: &Path) -> Cost {
    let Some((session, unit)) = compile_corpus_script(path) else {
        return Cost {
            fuel: None,
            outcome: Outcome::CompileError,
        };
    };
    measure(session.context(), &unit)
}

/// Compile and measure a source string against `ctx` — for the cases that
/// no corpus script can express.
fn measure_source(ctx: &Context, source: &str) -> Cost {
    let unit = ctx
        .compile(source)
        .unwrap_or_else(|e| panic!("case does not compile: {e}\n--- source ---\n{source}"));
    measure(ctx, &unit)
}

// ------------------------------------------------------- host re-entry

/// Host functions that call back into the VM through a [`ScriptClosure`] —
/// the re-entry path, where the dispatch loop must hand its fuel tank to
/// the nested run and take back what is left, on the success path *and* on
/// the error path.
///
/// No corpus script can reach this: the stdlib registers no closure-taking
/// host function, so re-entry is only expressible from an embedding. These
/// mirror the module `wscript/tests/interop.rs` uses for the same paths.
fn callbacks() -> Module {
    let mut m = Module::new("cbs");
    // Calls the closure twice; a callback fault propagates out of the host
    // call as a VM fault.
    m.fn_(
        "apply_twice",
        |ctx: &mut dyn HostCtx, cb: ScriptClosure<(i64,), i64>, x: i64| -> Fault<i64> {
            Fault(cb.call(ctx, (x,)).and_then(|y| cb.call(ctx, (y,))))
        },
    );
    // Catches a callback fault and recovers — the script keeps running, so
    // the tank has to have come back from the failed nested run.
    m.fn_(
        "call_or",
        |ctx: &mut dyn HostCtx, cb: ScriptClosure<(i64,), i64>, dflt: i64| -> i64 {
            cb.call(ctx, (1,)).unwrap_or(dflt)
        },
    );
    m
}

/// Re-entry cases, recorded in the same table as the corpus.
const HOST_CASES: &[(&str, &str)] = &[
    (
        "host:callback_ok",
        "use cbs\nfn main() -> int { cbs::apply_twice(|x| x * 2, 5) }",
    ),
    (
        "host:callback_fault",
        "use cbs\nfn main() -> int { cbs::apply_twice(|x| x / 0, 5) }",
    ),
    (
        "host:callback_caught",
        "use cbs\nfn main() -> int { cbs::call_or(|x| x / 0, -7) }",
    ),
];

/// One recorded host case, by name — so a test names the case it measures
/// instead of indexing into [`HOST_CASES`], where a reordering would
/// silently retarget the assertion.
fn host_case(name: &str) -> &'static str {
    HOST_CASES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, source)| *source)
        .unwrap_or_else(|| panic!("no host case named `{name}`"))
}

fn host_context() -> Context {
    Context::new().module(callbacks())
}

// ---------------------------------------------------------- the table

const PREAMBLE: &str = "\
# Fuel charged by every case in the corpus, in dispatched instructions
# (PRD §5.2) — the metered dispatch loop charges a straight-line block at
# a time, so a number is an instruction count, not a count of the points
# where fuel was taken.
#
# Regenerate with `just fuel-regen`, then read the diff. A number moving is
# a behavioural change, not a detail: a host with a per-tick budget sees it
# as a script that suddenly faults, or one that suddenly does not.
#
# One line per case — `<name> <fuel> <outcome>`. Outcomes:
";

const HOST_SECTION: &str = "\
#
# Host re-entry: a host function calling back into the VM through a
# ScriptClosure, which no corpus script can reach — the stdlib registers no
# closure-taking host function. See `callbacks()` in fuel_snapshot.rs.
";

/// The outcome legend, written from the same list the table's tags come
/// from, so an outcome can never appear in the table unexplained.
fn legend() -> String {
    let width = Outcome::ALL
        .iter()
        .map(|o| o.tag().len())
        .max()
        .expect("at least one outcome");
    let mut out = String::new();
    for outcome in Outcome::ALL {
        let (tag, lines) = outcome.described();
        for (i, line) in lines.iter().enumerate() {
            let column = if i == 0 { tag } else { "" };
            out.push_str(&format!("#   {column:width$}  {line}\n"));
        }
    }
    out
}

fn render() -> String {
    let mut out = String::from(PREAMBLE);
    out.push_str(&legend());
    out.push('\n');
    for path in corpus() {
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        out.push_str(&measure_script(&path).line(&name));
    }
    out.push_str(HOST_SECTION);
    let ctx = host_context();
    for (name, source) in HOST_CASES {
        out.push_str(&measure_source(&ctx, source).line(name));
    }
    out
}

/// `name → the rest of its line`, for reporting *which* case moved.
fn entries(text: &str) -> Vec<(&str, &str)> {
    text.lines()
        .filter(|l| !l.trim_start().starts_with('#') && !l.trim().is_empty())
        .map(|l| l.split_once(' ').unwrap_or((l, "")))
        .collect()
}

/// One message per case that gained, lost or changed a cost.
fn differences(expected: &str, actual: &str) -> Vec<String> {
    let (before, after) = (entries(expected), entries(actual));
    let mut out = Vec::new();
    for (name, cost) in &after {
        match before.iter().find(|(n, _)| n == name) {
            Some((_, was)) if was != cost => out.push(format!("{name}: {was} -> {cost}")),
            Some(_) => {}
            None => out.push(format!("{name}: not recorded yet ({cost})")),
        }
    }
    for (name, cost) in &before {
        if !after.iter().any(|(n, _)| n == name) {
            out.push(format!("{name}: recorded ({cost}) but no longer a case"));
        }
    }
    out
}

#[test]
fn fuel_costs_match_the_snapshot() {
    let actual = render();
    let path = snapshot_path();
    if std::env::var_os("WSCRIPT_REGEN_FUEL").is_some() {
        std::fs::write(&path, &actual)
            .unwrap_or_else(|e| panic!("writing {}: {e}", path.display()));
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_default();
    if expected == actual {
        return;
    }
    let mut moved = differences(&expected, &actual);
    if moved.is_empty() {
        moved.push("no cost changed — the table's header or layout did".to_string());
    }
    panic!(
        "{} fuel cost(s) moved in {}:\n  {}\n\n\
         Fuel is observable behaviour: a host budget that held before may not hold now. \
         If the change is intended, run `just fuel-regen` and review the diff.",
        moved.len(),
        path.display(),
        moved.join("\n  ")
    );
}

// --------------------------------------------------------- coverage

/// A path through the VM that fuel is charged at, recognised in the
/// bytecode.
#[derive(Clone, Copy)]
enum FuelPath {
    /// A loop: a jump that goes backwards, charging the block behind it.
    BackwardJump,
    /// An `if`/`match` arm: a jump that goes forwards.
    ForwardJump,
    /// A call into another script function, and the return from it.
    ScriptCall,
    /// A host call — charged 1 for the dispatch; what the host does
    /// internally is not metered.
    HostCall,
    /// A structural comparison or deep clone, which charges per value
    /// visited rather than once for the instruction.
    /// `ValueEq`/`ValueCmp`/`DeepClone` are only emitted for genuinely
    /// structural operands — primitives compare through their own opcodes
    /// — so finding one means a container walk.
    ///
    /// The other `charge_structural` site, rendering a container
    /// (`fmt_value`), is invisible here: `print`/`str` compile to the same
    /// builtin whatever the operand's shape. It is covered by measurement
    /// instead, in `printing_a_container_charges_per_value`.
    StructuralCompare,
}

impl FuelPath {
    fn matches(self, instr: &Instr) -> bool {
        match (self, instr) {
            (
                FuelPath::BackwardJump,
                Instr::Jump { off }
                | Instr::JumpIfFalse { off, .. }
                | Instr::JumpIfTrue { off, .. },
            ) => *off < 0,
            (
                FuelPath::ForwardJump,
                Instr::Jump { off }
                | Instr::JumpIfFalse { off, .. }
                | Instr::JumpIfTrue { off, .. },
            ) => *off > 0,
            (FuelPath::ScriptCall, Instr::CallValue { .. } | Instr::CallVirtual { .. }) => true,
            (
                FuelPath::ScriptCall,
                Instr::Call {
                    target: CallTarget::Proto(_),
                    ..
                },
            ) => true,
            (
                FuelPath::HostCall,
                Instr::Call {
                    target: CallTarget::Host(_),
                    ..
                },
            ) => true,
            (
                FuelPath::StructuralCompare,
                Instr::Call {
                    target: CallTarget::Builtin(b),
                    ..
                },
            ) => matches!(b, Builtin::ValueEq | Builtin::ValueCmp | Builtin::DeepClone),
            _ => false,
        }
    }
}

fn compiles_path(unit: &CompiledUnit, path: FuelPath) -> bool {
    unit.protos
        .iter()
        .any(|p| p.code.iter().any(|i| path.matches(i)))
}

/// A charged path, what to call it in a failure, and the corpus script
/// that carries it — one entry per path, so a path is described in exactly
/// one place.
struct Covered {
    path: FuelPath,
    what: &'static str,
    script: &'static str,
}

/// The corpus script that carries each path, named so a reader can see at a
/// glance what the table covers and so removing that script fails loudly
/// rather than quietly narrowing the snapshot.
///
/// This asserts the path is *compiled into* the named script, which the
/// snapshot then runs end to end — not that the specific instruction was
/// reached. Proving reachability would mean instrumenting the VM; the
/// scripts below are ones whose expected output cannot be produced without
/// taking the path, so the pairing is checked by `scripts.rs` too.
const COVERAGE: &[Covered] = &[
    Covered {
        path: FuelPath::BackwardJump,
        what: "a backward jump (loop)",
        script: "m2_for_loops",
    },
    Covered {
        path: FuelPath::ForwardJump,
        what: "a forward jump (if/match)",
        script: "m1_control_flow",
    },
    Covered {
        path: FuelPath::ScriptCall,
        what: "a script call and return",
        script: "m1_functions",
    },
    Covered {
        path: FuelPath::HostCall,
        what: "a host call",
        script: "m5_math",
    },
    Covered {
        path: FuelPath::StructuralCompare,
        what: "a structural comparison or deep clone",
        script: "m3_derives",
    },
];

#[test]
fn every_charged_path_is_covered_by_a_named_script() {
    let units: Vec<(String, CompiledUnit)> = corpus()
        .iter()
        .filter_map(|path| {
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            let (_, unit) = compile_corpus_script(path)?;
            Some((name, unit))
        })
        .collect();

    for covered in COVERAGE {
        let (_, unit) = units
            .iter()
            .find(|(name, _)| name == covered.script)
            .unwrap_or_else(|| panic!("no corpus script named `{}`", covered.script));
        if compiles_path(unit, covered.path) {
            continue;
        }
        let alternatives: Vec<&str> = units
            .iter()
            .filter(|(_, u)| compiles_path(u, covered.path))
            .map(|(name, _)| name.as_str())
            .collect();
        panic!(
            "`{}` no longer covers {}. Scripts that do: {}",
            covered.script,
            covered.what,
            if alternatives.is_empty() {
                "none — add one".to_string()
            } else {
                alternatives.join(", ")
            }
        );
    }
}

/// How many elements the printed list carries. Big enough that the walk
/// dwarfs the one instruction the two cases differ by.
const PRINTED_ELEMENTS: usize = 32;

/// Rendering a container charges per value visited (`fmt_value`'s
/// `charge_structural`) — the third structural example the ticket names,
/// and the one no bytecode check can see, since `println(xs)` and
/// `println(xs[0])` compile to the same builtin.
///
/// Measure it instead: both cases build the *same* list, so the only
/// difference between them is what gets rendered. A flat charge for the
/// call would make the two costs differ by a constant; charging per value
/// makes them differ by the length of the list.
#[test]
fn printing_a_container_charges_per_value() {
    let ctx = Context::new();
    let elements = (1..=PRINTED_ELEMENTS)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let whole = measure_source(
        &ctx,
        &format!("fn main() {{\n let xs = [{elements}]\n println(xs)\n}}"),
    );
    let element = measure_source(
        &ctx,
        &format!("fn main() {{\n let xs = [{elements}]\n println(xs[0])\n}}"),
    );

    assert_eq!(whole.outcome, Outcome::Ok);
    assert_eq!(element.outcome, Outcome::Ok);
    let walk = whole.fuel.unwrap().saturating_sub(element.fuel.unwrap());
    assert!(
        walk >= (PRINTED_ELEMENTS / 2) as u64,
        "printing a {PRINTED_ELEMENTS}-element list cost only {walk} more fuel than printing \
         one of its elements — rendering is not charged per value visited"
    );
}

/// Re-entry is covered by measurement rather than by reading the bytecode:
/// the closure body's own instructions have to show up in the bill, which
/// can only happen if the dispatch loop handed its tank to the nested run
/// and took back what was left.
#[test]
fn host_re_entry_charges_the_callback_body() {
    let ctx = host_context();
    let short = measure_source(
        &ctx,
        "use cbs\nfn main() -> int { cbs::apply_twice(|x| x, 5) }",
    );
    let long = measure_source(
        &ctx,
        "use cbs\nfn main() -> int { cbs::apply_twice(|x| x + 0 + 0 + 0 + 0, 5) }",
    );
    assert_eq!(short.outcome, Outcome::Ok);
    assert!(
        long.fuel > short.fuel,
        "a longer callback body cost no more fuel ({:?} vs {:?}) — \
         re-entry is not metering the nested run",
        long.fuel,
        short.fuel
    );

    // The error path owes the same sync: the host caught the callback's
    // fault, so the script ran on and its remaining cost was still charged.
    let caught = measure_source(&ctx, host_case("host:callback_caught"));
    assert_eq!(
        caught.outcome,
        Outcome::Ok,
        "the host recovers from the fault"
    );
    assert!(caught.fuel.unwrap() > 0);
}
