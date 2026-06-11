//! `#[derive(Script)]` — implemented in milestone M4 (interop).

use proc_macro::TokenStream;

/// Placeholder derive; the real implementation lands with the interop
/// milestone (PRD §6.2).
#[proc_macro_derive(Script, attributes(script))]
pub fn derive_script(_input: TokenStream) -> TokenStream {
    TokenStream::new()
}
