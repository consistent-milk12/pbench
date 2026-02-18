//! Proc macros for pbench benchmark registration.
//!
//! Provides `#[bench]` and `#[bench_group]` attribute macros that generate
//! static registration code using linker sections for pre-main initialization.

mod attr_options;
mod tokens;

use attr_options::{AttrOptions, Macro};
use proc_macro::TokenStream;
use proc_macro2::TokenStream as QuoteStream;
use quote::{format_ident, quote};
use syn::{ItemFn, ItemMod, parse_macro_input};

/// Platform-specific linker section attributes for pre-main registration.
///
/// Generates `#[used]` + `#[cfg_attr(..., link_section = "...")]` for each
/// supported platform family (ELF, Mach-O, Windows).
fn pre_main_attrs() -> QuoteStream {
    let elf_targets: QuoteStream = quote! {
        target_os = "linux",
        target_os = "android",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "fuchsia",
        target_os = "haiku",
        target_os = "illumos",
        target_os = "netbsd",
        target_os = "openbsd"
    };

    let mach_o_targets: QuoteStream = quote! {
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos"
    };

    quote! {
        #[used]
        #[cfg_attr(
            any(#elf_targets),
            unsafe(link_section = ".init_array")
        )]
        #[cfg_attr(
            any(#mach_o_targets),
            unsafe(link_section = "__DATA,__mod_init_func")
        )]
        #[cfg_attr(target_os = "windows", unsafe(link_section = ".CRT$XCU"))]
    }
}

/// Compile error for unsupported platforms.
fn unsupported_platform_check() -> QuoteStream {
    quote! {
        #[cfg(not(any(
            target_os = "linux",
            target_os = "android",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "fuchsia",
            target_os = "haiku",
            target_os = "illumos",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "macos",
            target_os = "ios",
            target_os = "tvos",
            target_os = "watchos",
            target_os = "windows",
        )))]
        compile_error!("pbench: unsupported target OS for benchmark registration");
    }
}

/// Generate the `EntryMeta { ... }` expression for a given identifier.
///
/// Uses unprefixed compiler built-in macros (`stringify!`, `module_path!`,
/// `file!`, `line!`, `column!`) which are always available without imports.
fn entry_meta_expr(private_mod: &QuoteStream, name_ident: &syn::Ident) -> QuoteStream {
    quote! {
        #private_mod::EntryMeta {
            raw_name: stringify!(#name_ident),
            module_path: module_path!(),
            location: #private_mod::EntryLocation {
                file: file!(),
                line: line!(),
                col: column!(),
            },
        }
    }
}

/// Mark a function as a benchmark.
///
/// # Supported signatures
///
/// ```ignore
/// #[pbench::bench]
/// fn no_args() { /* ... */ }
///
/// #[pbench::bench]
/// fn with_bencher(b: &Bencher) { /* ... */ }
///
/// #[pbench::bench(args = [1, 2, 4, 8])]
/// fn with_args(b: &Bencher, arg: &str) { /* ... */ }
/// ```
///
/// # Options
///
/// - `sample_count = <expr>` — number of samples
/// - `sample_size = <expr>` — iterations per sample
/// - `min_time = <seconds>` — minimum benchmark duration (f64 seconds)
/// - `max_time = <seconds>` — maximum benchmark duration (f64 seconds)
/// - `skip_ext_time` — skip external time in min/max accounting
/// - `args = [<exprs>]` — runtime arguments (requires 2-param fn signature)
/// - `ignore` — skip this benchmark unless `--include-ignored` is passed
#[proc_macro_attribute]
pub fn bench(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn: ItemFn = parse_macro_input!(item as ItemFn);
    let fn_name: &syn::Ident = &input_fn.sig.ident;
    let param_count: usize = input_fn.sig.inputs.len();

    // Check for #[ignore] attribute on the function.
    let ignore_attr: Option<syn::Path> = input_fn
        .attrs
        .iter()
        .find(|a: &&syn::Attribute| a.path().is_ident("ignore"))
        .map(|a: &syn::Attribute| a.path().clone());

    let options: AttrOptions = match AttrOptions::parse(attr, Macro::Bench { param_count }) {
        Ok(opts) => opts,
        Err(err) => return err,
    };

    let private_mod: &QuoteStream = &options.private_mod;
    let pre_main: QuoteStream = pre_main_attrs();
    let platform_check: QuoteStream = unsupported_platform_check();
    let meta_expr: QuoteStream = entry_meta_expr(private_mod, fn_name);

    // Generate unique static identifier from function name.
    let static_name: syn::Ident =
        format_ident!("__PBENCH_REGISTER_{}", fn_name.to_string().to_uppercase());

    let registration: QuoteStream = if let Some(ref args_array) = options.args_array {
        // args = [...] variant → GenericBenchEntry.
        //
        // Each array element is stringified at compile time via
        // `stringify!()`. The user function signature must be:
        //   fn name(b: &Bencher, arg: &str)
        let args_stringified: Vec<QuoteStream> = args_array
            .elems
            .iter()
            .map(|elem: &syn::Expr| quote! { stringify!(#elem) })
            .collect();

        quote! {
            {
                static ENTRY: #private_mod::GenericBenchEntry =
                    #private_mod::GenericBenchEntry {
                        meta: #meta_expr,
                        bench_fn: #fn_name,
                        args: &[#(#args_stringified),*],
                    };

                static ANY_ENTRY: #private_mod::AnyBenchEntry =
                    #private_mod::AnyBenchEntry::Generic(&ENTRY);

                static NODE: #private_mod::EntryList<
                    #private_mod::AnyBenchEntry,
                > = #private_mod::EntryList::new(&ANY_ENTRY);

                extern "C" fn push() {
                    #private_mod::BENCH_ENTRIES.push(&NODE);
                }

                #platform_check

                #pre_main
                static __PBENCH_PUSH: extern "C" fn() = push;
            }
        }
    } else {
        // Plain benchmark — no args.
        let bench_options_expr: QuoteStream = options.bench_options_fn(ignore_attr.as_ref());

        let bench_fn_expr: QuoteStream = match param_count {
            // fn my_bench() { ... }
            // → wrap in a named function that calls bench_refs
            0 => quote! {
                {
                    fn __pbench_wrap(
                        __b: &'_ #private_mod::Bencher<'_>,
                    ) {
                        __b.bench_refs(#fn_name);
                    }

                    __pbench_wrap
                }
            },

            // fn my_bench(b: &Bencher) { ... }
            // → use directly as BenchFn
            1 => quote! { #fn_name },

            _ => {
                return syn::Error::new_spanned(
                    &input_fn.sig,
                    "benchmark function must take 0 or 1 parameters \
                         (or use `args = [...]` for 2)",
                )
                .into_compile_error()
                .into();
            }
        };

        quote! {
            {
                static BENCH_ENTRY: #private_mod::BenchEntry =
                    #private_mod::BenchEntry {
                        meta: #meta_expr,
                        bench_fn: #bench_fn_expr,
                        options: #bench_options_expr,
                    };

                static ANY_ENTRY: #private_mod::AnyBenchEntry =
                    #private_mod::AnyBenchEntry::Bench(&BENCH_ENTRY);

                static NODE: #private_mod::EntryList<
                    #private_mod::AnyBenchEntry,
                > = #private_mod::EntryList::new(&ANY_ENTRY);

                extern "C" fn push() {
                    #private_mod::BENCH_ENTRIES.push(&NODE);
                }

                #platform_check

                #pre_main
                static __PBENCH_PUSH: extern "C" fn() = push;
            }
        }
    };

    let output: QuoteStream = quote! {
        #input_fn

        #[doc(hidden)]
        #[allow(non_upper_case_globals)]
        const #static_name: () = #registration;
    };

    output.into()
}

/// Mark a module as a benchmark group.
///
/// Groups provide hierarchical organisation and options inheritance.
/// A group's options cascade to all child benchmarks during resolution.
///
/// # Example
///
/// ```ignore
/// #[pbench::bench_group(sample_count = 500)]
/// mod my_group {
///     use super::*;
///
///     #[pbench::bench]
///     fn bench_a(b: &Bencher) { /* ... */ }
/// }
/// ```
#[proc_macro_attribute]
pub fn bench_group(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_mod: ItemMod = parse_macro_input!(item as ItemMod);
    let mod_name: &syn::Ident = &input_mod.ident;

    let options: AttrOptions = match AttrOptions::parse(attr, Macro::Group) {
        Ok(opts) => opts,
        Err(err) => return err,
    };

    let private_mod: &QuoteStream = &options.private_mod;
    let pre_main: QuoteStream = pre_main_attrs();
    let platform_check: QuoteStream = unsupported_platform_check();
    let meta_expr: QuoteStream = entry_meta_expr(private_mod, mod_name);
    let bench_options_expr: QuoteStream = options.bench_options_fn(None);

    let static_name: syn::Ident =
        format_ident!("__PBENCH_GROUP_{}", mod_name.to_string().to_uppercase());

    let registration: QuoteStream = quote! {
        {
            static GROUP_ENTRY: #private_mod::GroupEntry =
                #private_mod::GroupEntry {
                    meta: #meta_expr,
                    options: #bench_options_expr,
                };

            static ANY_ENTRY: #private_mod::AnyBenchEntry =
                #private_mod::AnyBenchEntry::Group(&GROUP_ENTRY);

            static NODE: #private_mod::EntryList<
                #private_mod::AnyBenchEntry,
            > = #private_mod::EntryList::new(&ANY_ENTRY);

            extern "C" fn push() {
                #private_mod::BENCH_ENTRIES.push(&NODE);
            }

            #platform_check

            #pre_main
            static __PBENCH_PUSH: extern "C" fn() = push;
        }
    };

    let output: QuoteStream = quote! {
        #input_mod

        #[doc(hidden)]
        #[allow(non_upper_case_globals)]
        const #static_name: () = #registration;
    };

    output.into()
}
