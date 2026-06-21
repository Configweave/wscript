//! `math` — pure numeric helpers (PRD §7). Always safe; no capabilities.
//!
//! Float functions take/return `float`; the `i*` variants work on `int`
//! (wscript has no overloading — user functions are monomorphic, PRD §3.6).

use std::sync::atomic::{AtomicU64, Ordering};

use wscript_core::Module;

/// Process-wide splitmix64 state for `rand()` (no external dependency;
/// seeded from the clock on first use).
static RNG_STATE: AtomicU64 = AtomicU64::new(0);

fn next_u64() -> u64 {
    let mut cur = RNG_STATE.load(Ordering::Relaxed);
    if cur == 0 {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E3779B97F4A7C15)
            | 1;
        let _ = RNG_STATE.compare_exchange(0, seed, Ordering::Relaxed, Ordering::Relaxed);
        cur = RNG_STATE.load(Ordering::Relaxed);
    }
    loop {
        let next = cur.wrapping_add(0x9E3779B97F4A7C15);
        match RNG_STATE.compare_exchange_weak(cur, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => {
                let mut z = next;
                z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
                z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
                return z ^ (z >> 31);
            }
            Err(actual) => cur = actual,
        }
    }
}

pub fn math() -> Module {
    let mut m = Module::new("math");
    m.doc("Pure numeric helpers; always safe to register");

    // float ops
    m.fn_("abs", |x: f64| x.abs());
    m.fn_("min", |a: f64, b: f64| a.min(b));
    m.fn_("max", |a: f64, b: f64| a.max(b));
    m.fn_("clamp", |x: f64, lo: f64, hi: f64| x.clamp(lo, hi));
    m.fn_("floor", |x: f64| x.floor());
    m.fn_("ceil", |x: f64| x.ceil());
    m.fn_("round", |x: f64| x.round());
    m.fn_("trunc", |x: f64| x.trunc());
    m.fn_("sqrt", |x: f64| x.sqrt());
    m.fn_("pow", |x: f64, y: f64| x.powf(y));
    m.fn_("exp", |x: f64| x.exp());
    m.fn_("ln", |x: f64| x.ln());
    m.fn_("log2", |x: f64| x.log2());
    m.fn_("log10", |x: f64| x.log10());
    m.fn_("sin", |x: f64| x.sin());
    m.fn_("cos", |x: f64| x.cos());
    m.fn_("tan", |x: f64| x.tan());
    m.fn_("asin", |x: f64| x.asin());
    m.fn_("acos", |x: f64| x.acos());
    m.fn_("atan", |x: f64| x.atan());
    m.fn_("atan2", |y: f64, x: f64| y.atan2(x));
    m.fn_("sinh", |x: f64| x.sinh());
    m.fn_("cosh", |x: f64| x.cosh());
    m.fn_("tanh", |x: f64| x.tanh());
    m.fn_("asinh", |x: f64| x.asinh());
    m.fn_("acosh", |x: f64| x.acosh());
    m.fn_("atanh", |x: f64| x.atanh());
    m.fn_("lerp", |a: f64, b: f64, t: f64| a + (b - a) * t);
    m.fn_("signum", |x: f64| x.signum());

    // int ops
    m.fn_("iabs", |x: i64| x.wrapping_abs());
    m.fn_("imin", |a: i64, b: i64| a.min(b));
    m.fn_("imax", |a: i64, b: i64| a.max(b));
    m.fn_("iclamp", |x: i64, lo: i64, hi: i64| x.clamp(lo, hi.max(lo)));
    m.fn_("isignum", |x: i64| x.signum());

    // consts
    m.const_("PI", std::f64::consts::PI);
    m.const_("E", std::f64::consts::E);
    m.const_("TAU", std::f64::consts::TAU);
    m.const_("INF", f64::INFINITY);
    m.const_("NAN", f64::NAN);

    // randomness
    m.doc_next("Uniform float in [0, 1)");
    m.fn_("rand", || (next_u64() >> 11) as f64 / (1u64 << 53) as f64);
    m.doc_next("Uniform int in [a, b) — empty ranges return a");
    m.fn_("rand_range", |a: i64, b: i64| {
        if b <= a {
            a
        } else {
            let span = (b - a) as u64;
            a + (next_u64() % span) as i64
        }
    });
    m
}
