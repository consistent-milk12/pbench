//! Attribute option parsing for `#[pbench::bench]` and `#[pbench::bench_group]`.
//!
//! Extracts user-specified benchmark configuration from macro attributes
//! and produces token streams for `BenchOptions` construction.

#![expect(clippy::option_if_let_else, reason = "Stylistic")]

use proc_macro::TokenStream;
use proc_macro2::TokenStream as QuoteStream;
use quote::{ToTokens, quote};
use syn::{
    Expr, ExprArray, ExprLit, Ident, LitBool, Path, meta as SynMeta, meta::ParseNestedMeta,
    parse::Parser, spanned::Spanned,
};

use crate::tokens::Tokenizer;

/// Which macro is being processed.
#[derive(Clone, Copy)]
pub enum Macro {
    /// `#[pbench::bench]` with the number of function parameters.
    Bench {
        /// Number of parameters in the annotated function.
        param_count: usize,
    },

    /// `#[pbench::bench_group]`.
    Group,
}

impl Macro {
    /// Display name for error messages.
    const fn name(self) -> &'static str {
        match self {
            Self::Bench { .. } => "bench",

            Self::Group => "bench_group",
        }
    }
}

/// Parsed attribute options shared between `#[bench]` and `#[bench_group]`.
pub struct AttrOptions {
    /// `pbench::__private` path.
    pub private_mod: QuoteStream,

    /// Array literal expression for runtime arguments (e.g. `[1, 2, 4, 8]`).
    pub args_array: Option<ExprArray>,

    /// Options used directly as `BenchOptions` fields.
    pub bench_options: Vec<(Ident, Expr)>,
}

impl AttrOptions {
    /// Parse attribute tokens into structured options.
    ///
    /// Returns `Err(TokenStream)` containing a compile error on failure.
    pub fn parse(tokens: TokenStream, target_macro: Macro) -> Result<Self, TokenStream> {
        let macro_name: &str = target_macro.name();

        let mut pbench_crate: Option<Path> = None::<Path>;
        let mut args_array: Option<ExprArray> = None::<ExprArray>;
        let mut bench_options: Vec<(Ident, Expr)> = Vec::new();

        let attr_parser = SynMeta::parser(|meta: ParseNestedMeta<'_>| {
            macro_rules! error {
                ($($t:tt)+) => {
                    return Err(meta.error(format_args!($($t)+)))
                };
            }

            let Some(ident) = meta.path.get_ident() else {
                error!("unsupported '{macro_name}' option");
            };

            let ident_name: String = ident.to_string();
            let ident_name: &str = ident_name.strip_prefix("r#").unwrap_or(&ident_name);

            let repeat_error = || error!("repeated '{macro_name}' option '{ident_name}'");

            macro_rules! parse_once {
                ($storage:expr) => {
                    if $storage.is_none() {
                        $storage = Some(meta.value()?.parse()?);
                    } else {
                        return repeat_error();
                    }
                };
            }

            match ident_name {
                "crate" => parse_once!(pbench_crate),

                "args" => {
                    match target_macro {
                        Macro::Bench { param_count } => {
                            if !matches!(param_count, 1 | 2) {
                                return Err(meta.error(format_args!(
                                    "function argument required for \
                                     '{macro_name}' option '{ident_name}'"
                                )));
                            }
                        }

                        Macro::Group => {
                            error!("unsupported '{macro_name}' option '{ident_name}'");
                        }
                    }

                    if args_array.is_some() {
                        return repeat_error();
                    }

                    let value: Expr = meta.value()?.parse()?;

                    match value {
                        Expr::Array(arr) => {
                            args_array = Some(arr);
                        }

                        _ => {
                            error!(
                                "'args' must be an array literal \
                                 (e.g. args = [1, 2, 3])"
                            );
                        }
                    }
                }

                _ => {
                    // All remaining options are forwarded as BenchOptions fields.
                    // Unknown fields will produce a compile error when assigned
                    // to `BenchOptions { field: ... }` in the generated code.
                    let value: Expr = match meta.value() {
                        Ok(value) => value.parse()?,

                        // Missing `=` → use `true` literal
                        // (for `ignore`, `skip_ext_time`).
                        Err(_) => Expr::Lit(ExprLit {
                            lit: LitBool::new(true, meta.path.span()).into(),
                            attrs: Vec::new(),
                        }),
                    };

                    // Check for repeated options.
                    if bench_options
                        .iter()
                        .any(|(existing, _): &(Ident, Expr)| existing == ident)
                    {
                        return repeat_error();
                    }

                    bench_options.push((ident.clone(), value));
                }
            }

            Ok(())
        });

        match attr_parser.parse(tokens) {
            Ok(()) => {}

            Err(error) => return Err(error.into_compile_error().into()),
        }

        let pbench_crate: Path = pbench_crate.unwrap_or_else(|| syn::parse_quote!(::pbench));
        let private_mod: QuoteStream = quote! { #pbench_crate::__private };

        Ok(Self {
            private_mod,
            args_array,
            bench_options,
        })
    }

    /// Produce the `Option<fn() -> BenchOptions>` field value for an entry.
    ///
    /// Returns `None` token if no options were specified. Otherwise returns
    /// a block expression defining a named function and evaluating to
    /// `Some(fn_ptr)` — compatible with `static` initializers.
    pub fn bench_options_fn(&self, ignore_attr: Option<&Path>) -> QuoteStream {
        let private_mod: &QuoteStream = &self.private_mod;
        let option_some: QuoteStream = Tokenizer::option_some();

        if self.bench_options.is_empty() && ignore_attr.is_none() {
            return Tokenizer::option_none();
        }

        let options_iter: Vec<QuoteStream> = self
            .bench_options
            .iter()
            .map(|(option, value): &(Ident, Expr)| {
                let option_name: String = option.to_string();
                let option_name: &str = option_name.strip_prefix("r#").unwrap_or(&option_name);

                let wrapped_value: QuoteStream;

                let value: &dyn ToTokens = match option_name {
                    // Duration fields: accept seconds as f64/u64 via
                    // `Duration::from_secs_f64`.
                    "min_time" | "max_time" => {
                        wrapped_value = quote! {
                            ::std::time::Duration::from_secs_f64(
                                #value as f64,
                            )
                        };

                        &wrapped_value
                    }

                    _ => value,
                };

                quote! { #option: #option_some(#value), }
            })
            .collect();

        let ignore: QuoteStream = match ignore_attr {
            Some(ignore_path) => {
                quote! { #ignore_path: #option_some(true), }
            }

            None => QuoteStream::new(),
        };

        // Generate a named function (not a closure) so the result is
        // usable in `static` initializers.
        quote! {
            {
                fn __pbench_options() -> #private_mod::BenchOptions {
                    #[allow(clippy::needless_update)]
                    #private_mod::BenchOptions {
                        #(#options_iter)*
                        #ignore
                        ..::std::default::Default::default()
                    }
                }

                #option_some(__pbench_options)
            }
        }
    }
}
