extern crate proc_macro;

use proc_macro::TokenStream;
use proc_macro2;
use proc_macro2::TokenStream as TokenStream2;
use syn;
use quote::{quote, ToTokens};
use std::str::FromStr;

#[proc_macro]
pub fn usrlib_boilerplate(_item: TokenStream) -> TokenStream {
    let output_tokens2 = quote!{
        #[pyclass]
        pub struct UsrFnLib {}
        #[pymethods]
        impl UsrFnLib {
            #[new]
            pub fn new() -> Self {
                Self {}
            }
        }
    };
    // println!("output_tokens: \n{}", output_tokens2.clone());
    TokenStream::from(output_tokens2)
}

fn lib_fn_macro_base(target_lib: &str, samp_type: &str, attr_tokens: TokenStream, input_tokens: TokenStream) -> TokenStream {
    let input_tokens2 = TokenStream2::from(input_tokens.clone());

    if cfg!(feature = "debug_token_print") {
        println!("\n======================================================================");
    }
    // println!("attr_tokens = {:#?}", attr_tokens);
    // println!("input_tokens = {:#?}", input_tokens);

    let parsed_struct = syn::parse_macro_input!(input_tokens as syn::ItemStruct);
    // println!("parsed_struct = {parsed_struct:#?}");

    let struct_ident = parsed_struct.ident.clone();
    if cfg!(feature = "debug_token_print") {
        println!("struct_ident = {struct_ident}");
    }

    let mut doc_tokens = TokenStream2::new();
    for attr_item in parsed_struct.attrs.iter() {
        if attr_item.path().is_ident("doc") {
            doc_tokens.extend(attr_item.to_token_stream())
        }
    };

    let mut field_idents = Vec::new();
    let mut field_ident_ty_tokens = Vec::new();
    for field in parsed_struct.fields.iter() {
        let ident_ = field.ident.clone().expect("Unnamed fields are not supported");
        let ty_ = field.ty.to_token_stream();
        let ident_ty_tokens = quote!{ #ident_ : #ty_ };

        field_idents.push(ident_);
        field_ident_ty_tokens.push(ident_ty_tokens);
    }
    // println!("field_idents = {field_idents:#?}");
    // println!("field_ident_ty_tokens = {field_ident_ty_tokens:#?}");

    let impl_pub_fn_new_tokens = quote!{
        impl #struct_ident {
            pub fn new(#(#field_ident_ty_tokens),*) -> Self {
                Self {#(#field_idents),*}
            }
        }
    };
    // println!("impl_pub_fn_new_tokens: \n{impl_pub_fn_new_tokens}\n");

    let pyo3_sig_tokens = if attr_tokens.is_empty() {
        quote!{#(#field_idents),*}
    } else {
        TokenStream2::from(attr_tokens)
    };
    // println!("pyo3_sig_tokens: \n{pyo3_sig_tokens}\n");

    let target_lib_tokens = TokenStream2::from_str(target_lib).unwrap();
    let fn_box_tokens = TokenStream2::from_str(
        &format!("FnBox{samp_type}")
    ).unwrap();
    // println!("targer_lib_tokens = {targer_lib_tokens}");
    // println!("fn_box_tokens = {fn_box_tokens}");

    let pymethods_impl_target_lib_tokens = quote!{
        #[pymethods]
        impl #target_lib_tokens {
            #[allow(non_snake_case)]
            #doc_tokens
            #[pyo3(signature = (#pyo3_sig_tokens))]
            pub fn #struct_ident(&self, #(#field_ident_ty_tokens),*) -> PyResult<#fn_box_tokens> {
                let fn_inst = #struct_ident::new(#(#field_idents),*);
                let fn_box = #fn_box_tokens { inner: Box::new(fn_inst)};
                Ok(fn_box)
            }
        }
    };
    // println!("pymethods_impl_target_lib_tokens: \n{pymethods_impl_target_lib_tokens}\n");

    let full_tokens = quote!{
        #[derive(Clone, Debug)]
        #input_tokens2

        #impl_pub_fn_new_tokens

        #pymethods_impl_target_lib_tokens
    };
    if cfg!(feature = "debug_token_print") {
        println!("full_tokens: \n{}\n", full_tokens);
    }

    // TokenStream::from(input_tokens2)
    TokenStream::from(full_tokens)
}

#[proc_macro_attribute]
pub fn usr_fn_f64(attr_tokens: TokenStream, input_tokens: TokenStream) -> TokenStream {
    lib_fn_macro_base("UsrFnLib", "F64", attr_tokens, input_tokens)
}

#[proc_macro_attribute]
pub fn usr_fn_bool(attr_tokens: TokenStream, input_tokens: TokenStream) -> TokenStream {
    lib_fn_macro_base("UsrFnLib", "Bool", attr_tokens, input_tokens)
}

#[proc_macro_attribute]
pub fn std_fn_f64(attr_tokens: TokenStream, input_tokens: TokenStream) -> TokenStream {
    lib_fn_macro_base("StdFnLib", "F64", attr_tokens, input_tokens)
}

#[proc_macro_attribute]
pub fn std_fn_bool(attr_tokens: TokenStream, input_tokens: TokenStream) -> TokenStream {
    lib_fn_macro_base("StdFnLib", "Bool", attr_tokens, input_tokens)
}