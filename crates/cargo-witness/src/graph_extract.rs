//! Owner: cargo-witness graph extraction
//! Proof: `cargo test -p cargo-witness`
//! Invariants: Public item extraction is deterministic and skips unreadable files.

use crate::model::PubItem;

/// Extract public items from a Rust source file using `syn`.
pub(crate) fn extract_pub_items(relative_path: &str, content: &str) -> Vec<PubItem> {
    let Ok(file) = syn::parse_file(content) else {
        // If parsing fails (e.g., the file has syntax errors), skip it.
        return Vec::new();
    };

    let mut items = Vec::new();

    for item in &file.items {
        match item {
            syn::Item::Fn(func) => {
                if matches!(func.vis, syn::Visibility::Public(_)) {
                    let signature = format_fn_signature(&func.sig);
                    items.push(PubItem {
                        kind: "fn".to_string(),
                        name: func.sig.ident.to_string(),
                        signature,
                    });
                }
            }
            syn::Item::Struct(s) => {
                if matches!(s.vis, syn::Visibility::Public(_)) {
                    let name = s.ident.to_string();
                    let generics = s.generics.params.iter().count();
                    let fields = match &s.fields {
                        syn::Fields::Named(named) => named.named.len(),
                        syn::Fields::Unnamed(unnamed) => unnamed.unnamed.len(),
                        syn::Fields::Unit => 0,
                    };
                    items.push(PubItem {
                        kind: "struct".to_string(),
                        name: name.clone(),
                        signature: format!(
                            "pub struct {name}{}({fields} fields)",
                            if generics > 0 {
                                format!("<{generics} generics>")
                            } else {
                                String::new()
                            }
                        ),
                    });
                }
            }
            syn::Item::Enum(e) => {
                if matches!(e.vis, syn::Visibility::Public(_)) {
                    let name = e.ident.to_string();
                    let variants = e.variants.len();
                    items.push(PubItem {
                        kind: "enum".to_string(),
                        name: name.clone(),
                        signature: format!("pub enum {name}({variants} variants)"),
                    });
                }
            }
            syn::Item::Trait(t) => {
                if matches!(t.vis, syn::Visibility::Public(_)) {
                    let name = t.ident.to_string();
                    let method_count = t
                        .items
                        .iter()
                        .filter(|item| matches!(item, syn::TraitItem::Fn(_)))
                        .count();
                    items.push(PubItem {
                        kind: "trait".to_string(),
                        name: name.clone(),
                        signature: format!("pub trait {name}({method_count} methods)"),
                    });
                }
            }
            syn::Item::Type(t) => {
                if matches!(t.vis, syn::Visibility::Public(_)) {
                    let name = t.ident.to_string();
                    items.push(PubItem {
                        kind: "type".to_string(),
                        name: name.clone(),
                        signature: format!("pub type {name} = ..."),
                    });
                }
            }
            syn::Item::Const(c) => {
                if matches!(c.vis, syn::Visibility::Public(_)) {
                    let name = c.ident.to_string();
                    items.push(PubItem {
                        kind: "const".to_string(),
                        name: name.clone(),
                        signature: format!("pub const {name}: ..."),
                    });
                }
            }
            syn::Item::Static(s) => {
                if matches!(s.vis, syn::Visibility::Public(_)) {
                    let name = s.ident.to_string();
                    items.push(PubItem {
                        kind: "static".to_string(),
                        name: name.clone(),
                        signature: format!("pub static {name}: ..."),
                    });
                }
            }
            syn::Item::Mod(m) => {
                if matches!(m.vis, syn::Visibility::Public(_)) {
                    let name = m.ident.to_string();
                    items.push(PubItem {
                        kind: "mod".to_string(),
                        name: name.clone(),
                        signature: format!("pub mod {name}"),
                    });
                }
            }
            syn::Item::Impl(imp) => {
                for impl_item in &imp.items {
                    if let syn::ImplItem::Fn(method) = impl_item
                        && matches!(method.vis, syn::Visibility::Public(_))
                    {
                        let self_ty = quote_type(&imp.self_ty);
                        let sig = format_fn_signature(&method.sig);
                        items.push(PubItem {
                            kind: "fn".to_string(),
                            name: method.sig.ident.to_string(),
                            signature: format!("impl {self_ty} :: {sig}"),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    items.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.name.cmp(&b.name)));

    let _ = relative_path;
    items
}

fn format_fn_signature(sig: &syn::Signature) -> String {
    let asyncness = if sig.asyncness.is_some() {
        "async "
    } else {
        ""
    };
    let unsafety = if sig.unsafety.is_some() {
        "unsafe "
    } else {
        ""
    };
    let name = &sig.ident;

    let generics = if sig.generics.params.is_empty() {
        String::new()
    } else {
        let params: Vec<String> = sig
            .generics
            .params
            .iter()
            .map(|param| match param {
                syn::GenericParam::Type(t) => t.ident.to_string(),
                syn::GenericParam::Lifetime(l) => format!("'{}", l.lifetime.ident),
                syn::GenericParam::Const(c) => format!("const {}", c.ident),
            })
            .collect();
        format!("<{}>", params.join(", "))
    };

    let inputs: Vec<String> = sig
        .inputs
        .iter()
        .map(|arg| match arg {
            syn::FnArg::Receiver(r) => {
                let prefix = if r.reference.is_some() {
                    if r.mutability.is_some() {
                        "&mut self"
                    } else {
                        "&self"
                    }
                } else {
                    "self"
                };
                prefix.to_string()
            }
            syn::FnArg::Typed(pat) => quote_type(&pat.ty),
        })
        .collect();

    let output = match &sig.output {
        syn::ReturnType::Default => String::new(),
        syn::ReturnType::Type(_, ty) => format!(" -> {}", quote_type(ty)),
    };

    format!(
        "{unsafety}{asyncness}fn {name}{generics}({}){output}",
        inputs.join(", ")
    )
}

fn quote_type(ty: &syn::Type) -> String {
    use quote::ToTokens;
    ty.to_token_stream().to_string()
}

#[cfg(test)]
#[path = "graph_extract_tests.rs"]
mod tests;
