// Copyright 2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/stylus-sdk-rs/blob/main/licenses/COPYRIGHT.md

use proc_macro2::TokenStream;
use quote::quote;
use syn::parse_quote;

use crate::types::Purity;

use super::types::{FnArgExtension, FnExtension, InterfaceExtension, PublicImpl};

#[derive(Debug)]
pub struct InterfaceAbi;

impl InterfaceExtension for InterfaceAbi {
    type FnExt = FnAbi;
    type Ast = syn::ItemImpl;

    fn build(_node: &syn::ItemImpl) -> Self {
        InterfaceAbi
    }

    fn codegen(iface: &PublicImpl<Self>) -> Self::Ast {
        let PublicImpl {
            generic_params,
            self_ty,
            where_clause,
            inheritance,
            funcs,
            ..
        } = iface;

        let name = match self_ty {
            syn::Type::Path(path) => path.path.segments.last().unwrap().ident.clone().to_string(),
            _ => todo!(),
        };

        let mut types = Vec::new();
        for item in funcs {
            if let Some(ty) = &item.extension.output {
                types.push(ty);
            }
        }
        let type_decls = quote! {
            let mut seen = HashSet::new();
            for item in ([] as [InnerType; 0]).iter() #(.chain(&<#types as InnerTypes>::inner_types()))* {
                if seen.insert(item.id) {
                    writeln!(f, "\n    {}", item.name)?;
                }
            }
        };

        // write the "is" clause in Solidity
        let mut is_clause = match inheritance.is_empty() {
            true => quote! {},
            false => quote! { write!(f, " is ")?; },
        };
        is_clause.extend(inheritance.iter().enumerate().map(|(i, ty)| {
            let comma = (i > 0).then_some(", ").unwrap_or_default();
            quote! {
                write!(f, "{}I{}", #comma, <#ty as GenerateAbi>::NAME)?;
            }
        }));

        let mut abi = TokenStream::new();
        for func in funcs {
            let sol_name = func.sol_name.to_string();
            let sol_args = func.inputs.iter().enumerate().map(|(i, arg)| {
                let comma = (i > 0).then_some(", ").unwrap_or_default();
                let name = arg.extension.pattern_ident.as_ref().map(ToString::to_string).unwrap_or_default();
                let ty = &arg.ty;
                quote! {
                    write!(f, "{}{}{}", #comma, <#ty as AbiType>::EXPORT_ABI_ARG, underscore_if_sol(#name))?;
                }
            });

            let sol_outs = if let Some(ty) = &func.extension.output {
                quote!(write_solidity_returns::<#ty>(f)?;)
            } else {
                quote!()
            };

            let sol_purity = match func.purity {
                Purity::Write => String::new(),
                x => format!(" {x}"),
            };

            abi.extend(quote! {
                write!(f, "\n    function {}(", #sol_name)?;
                #(#sol_args)*
                write!(f, ") external")?;
                write!(f, #sol_purity)?;
                #sol_outs
                writeln!(f, ";")?;
            });
        }

        parse_quote! {
            impl<#generic_params> stylus_sdk::abi::GenerateAbi for #self_ty where #where_clause {
                const NAME: &'static str = #name;

                fn fmt_abi(f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                    use stylus_sdk::abi::{AbiType, GenerateAbi};
                    use stylus_sdk::abi::internal::write_solidity_returns;
                    use stylus_sdk::abi::export::{underscore_if_sol, internal::{InnerType, InnerTypes}};
                    use std::collections::HashSet;
                    #(
                        <#inheritance as GenerateAbi>::fmt_abi(f)?;
                        writeln!(f)?;
                    )*
                    write!(f, "interface I{}", #name)?;
                    #is_clause
                    write!(f, "  {{")?;
                    #abi
                    #type_decls
                    writeln!(f, "}}")?;
                    Ok(())
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct FnAbi {
    pub output: Option<syn::Type>,
}

impl FnExtension for FnAbi {
    type FnArgExt = FnArgAbi;

    fn build(node: &syn::ImplItemFn) -> Self {
        let output = match &node.sig.output {
            syn::ReturnType::Default => None,
            syn::ReturnType::Type(_, ty) => Some(*ty.clone()),
        };
        FnAbi { output }
    }
}

#[derive(Debug)]
pub struct FnArgAbi {
    pub pattern_ident: Option<syn::Ident>,
}

impl FnArgExtension for FnArgAbi {
    fn build(node: &syn::FnArg) -> Self {
        let pattern_ident = if let syn::FnArg::Typed(pat_type) = node {
            pattern_ident(&pat_type.pat)
        } else {
            None
        };
        FnArgAbi { pattern_ident }
    }
}

/// finds the root type for a given arg
fn pattern_ident(pat: &syn::Pat) -> Option<syn::Ident> {
    match pat {
        syn::Pat::Ident(pat_ident) => Some(pat_ident.ident.clone()),
        syn::Pat::Reference(pat_ref) => pattern_ident(&pat_ref.pat),
        _ => None,
    }
}
