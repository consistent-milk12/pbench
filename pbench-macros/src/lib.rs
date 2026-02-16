use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn bench(_attr: TokenStream, _item: TokenStream) -> TokenStream {
    todo!("pbench::bench macro")
}

#[proc_macro_attribute]
pub fn bench_group(_attr: TokenStream, _item: TokenStream) -> TokenStream {
    todo!("pbench::bench_group macro")
}
