//! Will work on this later.

#![deny(missing_docs)]
#![warn(clippy::nursery)]
#![warn(clippy::perf)]
#![warn(clippy::pedantic)]
// Tested functions only
#![allow(clippy::inline_always)]

use proc_macro::TokenStream;

/// `pbench::bench`
#[proc_macro_attribute]
pub fn bench(_attr: TokenStream, _item: TokenStream) -> TokenStream {
    todo!("pbench::bench macro")
}

/// `pbench::bench_group`
#[proc_macro_attribute]
pub fn bench_group(_attr: TokenStream, _item: TokenStream) -> TokenStream {
    todo!("pbench::bench_group macro")
}
