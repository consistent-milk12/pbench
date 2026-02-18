//! Token generation utilities for macro hygiene.
//!
//! All standard library items use `::std` paths to avoid conflicts
//! with user imports. This works unless users write
//! `extern crate x as std`, which is extremely unlikely.

use proc_macro2::TokenStream as QuoteStream;
use quote::quote;

pub struct Tokenizer;

impl Tokenizer {
    /// Generate `::std::option::Option::Some`.
    #[must_use]
    pub fn option_some() -> QuoteStream {
        quote!(::std::option::Option::Some)
    }

    /// Generate `::std::option::Option::None`.
    #[must_use]
    pub fn option_none() -> QuoteStream {
        quote!(::std::option::Option::None)
    }
}
