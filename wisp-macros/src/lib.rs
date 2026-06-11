//! `#[derive(Script)]` (PRD §6.2): expose a Rust type to wisp scripts.
//!
//! - **Data types** (default): fields visible in script, constructible
//!   (`Point { x: 1, y: 2 }`) and destructurable in `match`; values convert
//!   *by value* across the boundary. Enums derive too, including payloads.
//! - **Handle types** (`#[script(opaque)]`): opaque resources — no field
//!   access, methods only (registered via `Module::ty::<T>().method(...)`);
//!   values cross *by handle*.
//!
//! Generated code references the `wisp` umbrella crate by default; inside
//! crates that depend on `wisp-core` directly, use
//! `#[script(crate_path = "wisp_core")]`.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Data, DeriveInput, Fields, parse_macro_input};

#[proc_macro_derive(Script, attributes(script))]
pub fn derive_script(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(input)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

struct Options {
    opaque: bool,
    /// Path to wisp-core (default goes through the `wisp` umbrella).
    core: TokenStream2,
    /// Script-visible name (defaults to the Rust name).
    name: Option<String>,
}

fn parse_options(input: &DeriveInput) -> syn::Result<Options> {
    let mut opts = Options {
        opaque: false,
        core: quote!(::wisp::core),
        name: None,
    };
    for attr in &input.attrs {
        if !attr.path().is_ident("script") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("opaque") {
                opts.opaque = true;
                Ok(())
            } else if meta.path.is_ident("crate_path") {
                let lit: syn::LitStr = meta.value()?.parse()?;
                let path: syn::Path = lit.parse()?;
                opts.core = quote!(#path);
                Ok(())
            } else if meta.path.is_ident("name") {
                let lit: syn::LitStr = meta.value()?.parse()?;
                opts.name = Some(lit.value());
                Ok(())
            } else {
                Err(meta.error(
                    "unknown #[script(...)] option; supported: opaque, crate_path, name",
                ))
            }
        })?;
    }
    Ok(opts)
}

fn expand(input: DeriveInput) -> syn::Result<TokenStream2> {
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "#[derive(Script)] types must be monomorphic (wisp v1 has no user generics)",
        ));
    }
    let opts = parse_options(&input)?;
    let ident = &input.ident;
    let script_name = opts.name.clone().unwrap_or_else(|| ident.to_string());
    let core = &opts.core;

    if opts.opaque {
        return expand_opaque(ident, &script_name, core, &input);
    }

    match &input.data {
        Data::Struct(data) => expand_struct(ident, &script_name, core, data),
        Data::Enum(data) => expand_enum(ident, &script_name, core, data),
        Data::Union(_) => Err(syn::Error::new_spanned(
            ident,
            "#[derive(Script)] does not support unions",
        )),
    }
}

fn expand_opaque(
    ident: &syn::Ident,
    name: &str,
    core: &TokenStream2,
    input: &DeriveInput,
) -> syn::Result<TokenStream2> {
    if matches!(&input.data, Data::Union(_)) {
        return Err(syn::Error::new_spanned(
            ident,
            "#[derive(Script)] does not support unions",
        ));
    }
    Ok(quote! {
        impl #core::host::ScriptType for #ident {
            fn script_type(defs: &mut #core::defs::DefTable) -> #core::types::Type {
                let id = #core::host::register_host_struct(
                    defs,
                    #name,
                    ::std::any::TypeId::of::<#ident>(),
                    true,
                    |_| ::std::vec::Vec::new(),
                );
                #core::types::Type::Named(id)
            }
        }

        impl #core::host::ScriptOpaque for #ident {}

        impl #core::host::IntoValue for #ident {
            fn into_value(
                self,
                defs: &#core::defs::DefTable,
            ) -> ::std::result::Result<#core::value::Value, #core::host::HostError> {
                #core::module::opaque_into_value(self, defs)
            }
        }
    })
}

fn expand_struct(
    ident: &syn::Ident,
    name: &str,
    core: &TokenStream2,
    data: &syn::DataStruct,
) -> syn::Result<TokenStream2> {
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new_spanned(
            ident,
            "#[derive(Script)] data structs need named fields \
             (tuple structs are not representable in wisp; use #[script(opaque)])",
        ));
    };
    let field_idents: Vec<&syn::Ident> =
        fields.named.iter().map(|f| f.ident.as_ref().unwrap()).collect();
    let field_names: Vec<String> = field_idents.iter().map(|i| i.to_string()).collect();
    let field_tys: Vec<&syn::Type> = fields.named.iter().map(|f| &f.ty).collect();
    let indices: Vec<usize> = (0..field_idents.len()).collect();
    let n_fields = field_idents.len();

    Ok(quote! {
        impl #core::host::ScriptType for #ident {
            fn script_type(defs: &mut #core::defs::DefTable) -> #core::types::Type {
                let id = #core::host::register_host_struct(
                    defs,
                    #name,
                    ::std::any::TypeId::of::<#ident>(),
                    false,
                    |defs| ::std::vec![
                        #((
                            #field_names.to_string(),
                            <#field_tys as #core::host::ScriptType>::script_type(defs),
                        )),*
                    ],
                );
                #core::types::Type::Named(id)
            }
        }

        impl #core::host::FromValue for #ident {
            fn from_value(
                v: #core::value::Value,
                defs: &#core::defs::DefTable,
            ) -> ::std::result::Result<Self, #core::host::HostError> {
                let id = #core::host::lookup_def::<#ident>(defs)?;
                match &v {
                    #core::value::Value::Struct(s) if s.def == id => {
                        let fields = s.fields.borrow();
                        if fields.len() != #n_fields {
                            return ::std::result::Result::Err(#core::host::HostError::msg(
                                "struct field count mismatch at host boundary",
                            ));
                        }
                        ::std::result::Result::Ok(#ident {
                            #(#field_idents: <#field_tys as #core::host::FromValue>::from_value(
                                fields[#indices].clone(),
                                defs,
                            )?,)*
                        })
                    }
                    other => ::std::result::Result::Err(#core::host::type_mismatch(
                        #name, other,
                    )),
                }
            }
        }

        impl #core::host::IntoValue for #ident {
            fn into_value(
                self,
                defs: &#core::defs::DefTable,
            ) -> ::std::result::Result<#core::value::Value, #core::host::HostError> {
                let id = #core::host::lookup_def::<#ident>(defs)?;
                ::std::result::Result::Ok(#core::value::Value::new_struct(
                    id,
                    ::std::vec![
                        #(<#field_tys as #core::host::IntoValue>::into_value(
                            self.#field_idents,
                            defs,
                        )?),*
                    ],
                ))
            }
        }
    })
}

fn expand_enum(
    ident: &syn::Ident,
    name: &str,
    core: &TokenStream2,
    data: &syn::DataEnum,
) -> syn::Result<TokenStream2> {
    let mut variant_defs = Vec::new();
    let mut from_arms = Vec::new();
    let mut into_arms = Vec::new();

    for (tag, variant) in data.variants.iter().enumerate() {
        let tag = tag as u32;
        let v_ident = &variant.ident;
        let v_name = v_ident.to_string();
        match &variant.fields {
            Fields::Unit => {
                variant_defs.push(quote! {
                    #core::defs::VariantDef {
                        name: #v_name.to_string(),
                        kind: #core::defs::VariantKind::Unit,
                        fields: ::std::vec::Vec::new(),
                    }
                });
                from_arms.push(quote! {
                    #tag => ::std::result::Result::Ok(#ident::#v_ident),
                });
                into_arms.push(quote! {
                    #ident::#v_ident => (#tag, ::std::vec::Vec::new()),
                });
            }
            Fields::Unnamed(fields) => {
                let tys: Vec<&syn::Type> = fields.unnamed.iter().map(|f| &f.ty).collect();
                let idents: Vec<syn::Ident> = (0..tys.len())
                    .map(|i| quote::format_ident!("p{i}"))
                    .collect();
                let positions: Vec<String> = (0..tys.len()).map(|i| i.to_string()).collect();
                let indices: Vec<usize> = (0..tys.len()).collect();
                variant_defs.push(quote! {
                    #core::defs::VariantDef {
                        name: #v_name.to_string(),
                        kind: #core::defs::VariantKind::Tuple,
                        fields: ::std::vec![
                            #((
                                #positions.to_string(),
                                <#tys as #core::host::ScriptType>::script_type(defs),
                            )),*
                        ],
                    }
                });
                from_arms.push(quote! {
                    #tag => {
                        let fields = e.fields.borrow();
                        ::std::result::Result::Ok(#ident::#v_ident(
                            #(<#tys as #core::host::FromValue>::from_value(
                                fields[#indices].clone(),
                                defs,
                            )?),*
                        ))
                    }
                });
                into_arms.push(quote! {
                    #ident::#v_ident(#(#idents),*) => (
                        #tag,
                        ::std::vec![
                            #(<#tys as #core::host::IntoValue>::into_value(#idents, defs)?),*
                        ],
                    ),
                });
            }
            Fields::Named(fields) => {
                let f_idents: Vec<&syn::Ident> =
                    fields.named.iter().map(|f| f.ident.as_ref().unwrap()).collect();
                let f_names: Vec<String> = f_idents.iter().map(|i| i.to_string()).collect();
                let f_tys: Vec<&syn::Type> = fields.named.iter().map(|f| &f.ty).collect();
                let indices: Vec<usize> = (0..f_idents.len()).collect();
                variant_defs.push(quote! {
                    #core::defs::VariantDef {
                        name: #v_name.to_string(),
                        kind: #core::defs::VariantKind::Struct,
                        fields: ::std::vec![
                            #((
                                #f_names.to_string(),
                                <#f_tys as #core::host::ScriptType>::script_type(defs),
                            )),*
                        ],
                    }
                });
                from_arms.push(quote! {
                    #tag => {
                        let fields = e.fields.borrow();
                        ::std::result::Result::Ok(#ident::#v_ident {
                            #(#f_idents: <#f_tys as #core::host::FromValue>::from_value(
                                fields[#indices].clone(),
                                defs,
                            )?,)*
                        })
                    }
                });
                into_arms.push(quote! {
                    #ident::#v_ident { #(#f_idents),* } => (
                        #tag,
                        ::std::vec![
                            #(<#f_tys as #core::host::IntoValue>::into_value(#f_idents, defs)?),*
                        ],
                    ),
                });
            }
        }
    }

    Ok(quote! {
        impl #core::host::ScriptType for #ident {
            fn script_type(defs: &mut #core::defs::DefTable) -> #core::types::Type {
                let id = #core::host::register_host_enum(
                    defs,
                    #name,
                    ::std::any::TypeId::of::<#ident>(),
                    |defs| ::std::vec![#(#variant_defs),*],
                );
                #core::types::Type::Named(id)
            }
        }

        impl #core::host::FromValue for #ident {
            fn from_value(
                v: #core::value::Value,
                defs: &#core::defs::DefTable,
            ) -> ::std::result::Result<Self, #core::host::HostError> {
                let id = #core::host::lookup_def::<#ident>(defs)?;
                match &v {
                    #core::value::Value::Enum(e) if e.def == id => match e.tag {
                        #(#from_arms)*
                        _ => ::std::result::Result::Err(#core::host::HostError::msg(
                            "unknown enum tag at host boundary",
                        )),
                    },
                    other => ::std::result::Result::Err(#core::host::type_mismatch(
                        #name, other,
                    )),
                }
            }
        }

        impl #core::host::IntoValue for #ident {
            fn into_value(
                self,
                defs: &#core::defs::DefTable,
            ) -> ::std::result::Result<#core::value::Value, #core::host::HostError> {
                let id = #core::host::lookup_def::<#ident>(defs)?;
                let (tag, fields) = match self {
                    #(#into_arms)*
                };
                ::std::result::Result::Ok(#core::value::Value::new_enum(id, tag, fields))
            }
        }
    })
}
