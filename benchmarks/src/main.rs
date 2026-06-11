//! Microbenchmarks: wisp vs Lua 5.4 (mlua), Rhai, Rune (PRD §10 M7).
//!
//! Honest numbers, whatever they are: each engine runs an equivalent
//! program; we time only execution (compilation excluded), run each
//! benchmark `RUNS` times and report the best wall-clock time. Run with
//! `cargo run --release -p wisp-benchmarks`.

use std::sync::Arc;
use std::time::{Duration, Instant};

const RUNS: usize = 5;

fn time_best(mut f: impl FnMut()) -> Duration {
    let mut best = Duration::MAX;
    for _ in 0..RUNS {
        let start = Instant::now();
        f();
        best = best.min(start.elapsed());
    }
    best
}

struct Bench {
    name: &'static str,
    expected: i64,
    wisp: &'static str,
    lua: &'static str,
    rhai: &'static str,
    rune: &'static str,
}

const BENCHES: &[Bench] = &[
    Bench {
        name: "fib(25) recursive",
        expected: 75025,
        wisp: "fn fib(n: int) -> int { if n < 2 { n } else { fib(n - 1) + fib(n - 2) } }\n\
               fn main() -> int { fib(25) }",
        lua: "local function fib(n) if n < 2 then return n end return fib(n-1) + fib(n-2) end\n\
              function main() return fib(25) end",
        rhai: "fn fib(n) { if n < 2 { n } else { fib(n - 1) + fib(n - 2) } }\n\
               fn main() { fib(25) }",
        rune: "fn fib(n) { if n < 2 { n } else { fib(n - 1) + fib(n - 2) } }\n\
               pub fn main() { fib(25) }",
    },
    Bench {
        name: "loop sum 1..=2_000_000",
        expected: 2_000_001_000_000,
        wisp: "fn main() -> int { let total = 0\n let i = 1\n while i <= 2000000 { total = total + i\n i = i + 1 }\n total }",
        lua: "function main() local total = 0\n for i = 1, 2000000 do total = total + i end\n return total end",
        rhai: "fn main() { let total = 0; let i = 1; while i <= 2000000 { total += i; i += 1; } total }",
        rune: "pub fn main() { let total = 0; let i = 1; while i <= 2000000 { total = total + i; i = i + 1; } total }",
    },
    Bench {
        name: "string build (20k pieces)",
        expected: 20000,
        wisp: "fn main() -> int { let parts = []\n let i = 0\n while i < 20000 { parts.push(str(i))\n i = i + 1 }\n parts.join(\",\").split(\",\").len() }",
        lua: "function main() local parts = {}\n for i = 0, 19999 do parts[#parts + 1] = tostring(i) end\n local s = table.concat(parts, \",\")\n local n = 0\n for _ in string.gmatch(s, '([^,]+)') do n = n + 1 end\n return n end",
        rhai: "fn main() { let parts = []; let i = 0; while i < 20000 { parts.push(i.to_string()); i += 1; } parts.reduce(|a, b| if a == () { b } else { a + \",\" + b }).split(\",\").len() }",
        rune: "pub fn main() { let parts = Vec::new(); let i = 0; while i < 20000 { parts.push(format!(\"{}\", i)); i = i + 1; } let joined = String::from_str(\"\"); for (n, p) in parts.iter().enumerate() { if n > 0 { joined.push_str(\",\"); } joined.push_str(p); } let count = 0; for _ in joined.split(',') { count = count + 1; } count }",
    },
    Bench {
        name: "list sort (10k ints)",
        expected: 9999,
        wisp: "fn main() -> int { let xs = []\n let i = 0\n while i < 10000 { xs.push((i * 2654435761) % 10000)\n i = i + 1 }\n xs.sort()\n xs[9999] - xs[0] }",
        lua: "function main() local xs = {}\n for i = 0, 9999 do xs[#xs + 1] = (i * 2654435761) % 10000 end\n table.sort(xs)\n return xs[10000] - xs[1] end",
        rhai: "fn main() { let xs = []; let i = 0; while i < 10000 { xs.push((i * 2654435761) % 10000); i += 1; } xs.sort(); xs[9999] - xs[0] }",
        rune: "pub fn main() { let xs = Vec::new(); let i = 0; while i < 10000 { xs.push((i * 2654435761) % 10000); i = i + 1; } xs.sort(); xs[9999] - xs[0] }",
    },
];

fn run_wisp(src: &str, expected: i64) -> Duration {
    let ctx = wisp::Context::new();
    let unit = ctx.compile(src).expect("wisp compile");
    let mut vm = wisp::Vm::new(&ctx);
    time_best(|| {
        let n: i64 = vm.call_unit(&unit, "main", ()).expect("wisp run");
        assert_eq!(n, expected, "wisp result");
    })
}

fn run_lua(src: &str, expected: i64) -> Duration {
    let lua = mlua::Lua::new();
    lua.load(src).exec().expect("lua load");
    let main: mlua::Function = lua.globals().get("main").expect("lua main");
    time_best(|| {
        let n: i64 = main.call(()).expect("lua run");
        assert_eq!(n, expected, "lua result");
    })
}

fn run_rhai(src: &str, expected: i64) -> Duration {
    let engine = rhai::Engine::new();
    let ast = engine.compile(src).expect("rhai compile");
    let mut scope = rhai::Scope::new();
    time_best(|| {
        let n: i64 = engine
            .call_fn(&mut scope, &ast, "main", ())
            .expect("rhai run");
        assert_eq!(n, expected, "rhai result");
    })
}

fn run_rune(src: &str, expected: i64) -> Duration {
    let context = rune::Context::with_default_modules().expect("rune ctx");
    let runtime = Arc::new(context.runtime().expect("rune runtime"));
    let mut sources = rune::Sources::new();
    sources
        .insert(rune::Source::memory(src).expect("rune source"))
        .expect("rune sources");
    let mut diagnostics = rune::Diagnostics::new();
    let unit = rune::prepare(&mut sources)
        .with_context(&context)
        .with_diagnostics(&mut diagnostics)
        .build()
        .expect("rune compile");
    let unit = Arc::new(unit);
    time_best(|| {
        let mut vm = rune::Vm::new(runtime.clone(), unit.clone());
        let output = vm.call(["main"], ()).expect("rune run");
        let n: i64 = rune::from_value(output).expect("rune result");
        assert_eq!(n, expected, "rune result");
    })
}

fn main() {
    println!("| benchmark | wisp | Lua 5.4 (mlua) | Rhai | Rune |\n|---|---|---|---|---|");
    for b in BENCHES {
        let wisp = run_wisp(b.wisp, b.expected);
        let lua = run_lua(b.lua, b.expected);
        let rhai = run_rhai(b.rhai, b.expected);
        let rune = run_rune(b.rune, b.expected);
        println!(
            "| {} | {:.2?} | {:.2?} | {:.2?} | {:.2?} |",
            b.name, wisp, lua, rhai, rune
        );
    }
    println!(
        "\nbest of {RUNS} runs each; execution only (compilation excluded); \
         release profile"
    );
}
