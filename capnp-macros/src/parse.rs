use syn::{
    Token, braced,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    token::Brace,
};

// {field1, field2, ...}
pub struct CapnpAnonStruct<FieldPattern: Parse> {
    pub _brace_token: Brace,
    pub fields: Punctuated<FieldPattern, Token![,]>,
}

impl<FieldPattern: Parse> Parse for CapnpAnonStruct<FieldPattern> {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        Ok(CapnpAnonStruct {
            _brace_token: braced!(content in input),
            //fields: syn::punctuated::Punctuated::parse_terminated(&content)?,
            fields: content.parse_terminated(FieldPattern::parse, Token![,])?,
        })
    }
}
