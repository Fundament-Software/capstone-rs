use crate::parse::{CapnpLetFieldPattern, CapnpLetStruct};
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::Ident;

/// Takes `expr` as an identifier of a capnproto Reader type of some struct and extracts fields specified in `pat`.
/// `pat` is of the form `{capnpfield1, capnpfield2, ...}`. Each `capnpfield` is a pair `lhs: rhs`.
/// Returns token stream of assignments for variables specified recursively in `rhs`.
pub fn process_let_pry(pat: CapnpLetStruct, expr: Ident) -> syn::Result<TokenStream2> {
    let mut res = TokenStream2::new();
    for field_pattern in pat.fields.into_iter() {
        let to_append = match field_pattern {
            CapnpLetFieldPattern::Name(name) => {
                let field_accessor = format_ident!("get_{}", name);
                quote! {
                    let #name = #expr.reborrow().#field_accessor();
                    let #name = capnp_rpc::pry!(#name.into_result());
                }
            }
            CapnpLetFieldPattern::ExtractToSymbol(name, symbol) => {
                let field_accessor = format_ident!("get_{}", name);
                quote! {
                    let #symbol = #expr.reborrow().#field_accessor();
                    let #symbol = capnp_rpc::pry!(#symbol.into_result());
                }
            }
            CapnpLetFieldPattern::ExtractWithPattern(name, struct_pattern) => {
                let field_accessor = format_ident!("get_{}", name);
                let head = quote! {
                    let #name = #expr.reborrow().#field_accessor();
                    let #name = capnp_rpc::pry!(#name.into_result());
                };
                let tail = process_let_pry(struct_pattern, name)?;
                quote!(#head #tail)
            }
        };
        //let CapnpField { lhs, rhs, .. } = field;
        // let field_accessor = format_ident!("get_{}", lhs);
        // let to_append = match *rhs {
        //     CapnpFieldPat::Ident(ident) => {
        //         quote! {
        //             let #ident = #expr.reborrow().#field_accessor();
        //             let #ident = capnp_rpc::pry!(#ident.into_result());
        //         }
        //     }
        //     CapnpFieldPat::AnonStruct(s) => {
        //         let head = quote! {
        //             let #lhs = #expr.reborrow().#field_accessor();
        //             let #lhs = capnp_rpc::pry!(#lhs.into_result());
        //         };
        //         let tail = process_let_pry(s, lhs)?;
        //         quote!(#head #tail)
        //     }
        // };
        //dbg!(&to_append.to_string());
        res.extend(to_append);
    }
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::CapnpLet;

    #[test]
    fn test_new() -> syn::Result<()> {
        let input = quote! {
            {name, birthdate: {year_as_text: year, month, day}, email: contactEmail} = person
        }; // person is person_capnp::person::Reader
        let CapnpLet {
            struct_pattern,
            ident,
            ..
        } = syn::parse2(input)?;
        process_let_pry(struct_pattern, ident)?;
        Ok(())
    }
}