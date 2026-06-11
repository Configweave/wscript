//! The wisp standard library (PRD §7).
//!
//! Every module is **opt-in at embed time** (capability-style — nothing is
//! ambient): `Context::new().module(wisp_std::math())`. Each module is also
//! feature-gated in Cargo so minimal embedders compile none of it. The CLI
//! enables everything by default.
//!
//! Surface note (PRD §7, decided): string operations live as *methods on
//! the `string` type* (`s.len()`, `s.split(",")` …) — one surface,
//! consistently — so there is no separate `string` module; the
//! formatting builtin is the prelude's `fmt(template, args…)`.

#[cfg(feature = "fs")]
mod fs_mod;
#[cfg(feature = "json")]
mod json_mod;
#[cfg(feature = "math")]
mod math_mod;
#[cfg(feature = "process")]
mod process_mod;
#[cfg(feature = "toml")]
mod toml_mod;
mod value;
#[cfg(feature = "xml")]
mod xml_mod;

pub use value::DynValue;
pub use value::value;

#[cfg(feature = "fs")]
pub use fs_mod::fs;
#[cfg(feature = "json")]
pub use json_mod::json;
#[cfg(feature = "math")]
pub use math_mod::math;
#[cfg(feature = "process")]
pub use process_mod::{Output, process, process_with_args};
#[cfg(feature = "toml")]
pub use toml_mod::toml;
#[cfg(feature = "xml")]
pub use xml_mod::xml;

/// All enabled stdlib modules (what `wisp run` registers by default).
/// `script_args` feeds `process::args()`.
pub fn all_modules(script_args: Vec<String>) -> Vec<wisp_core::Module> {
    #[allow(unused_mut)]
    let mut mods = vec![value()];
    #[cfg(feature = "math")]
    mods.push(math());
    #[cfg(feature = "fs")]
    mods.push(fs());
    #[cfg(feature = "process")]
    mods.push(process_with_args(script_args));
    #[cfg(not(feature = "process"))]
    let _ = script_args;
    #[cfg(feature = "json")]
    mods.push(json());
    #[cfg(feature = "toml")]
    mods.push(toml());
    #[cfg(feature = "xml")]
    mods.push(xml());
    mods
}
