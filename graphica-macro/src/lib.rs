use encase_derive_impl::implement;
use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use quote::quote;
use syn::{Data, DeriveInput, Fields, Path, parse_macro_input, parse_str};

fn get_crate_root() -> proc_macro2::TokenStream {
    match crate_name("graphics") {
        Ok(FoundCrate::Name(name)) => {
            let ident = syn::Ident::new(&name, proc_macro2::Span::call_site());
            quote!(::#ident)
        }
        _ => quote!(::graphics),
    }
}

fn check_repr_c(input: &DeriveInput) -> Result<(), syn::Error> {
    let mut has_repr_c = false;
    for attr in &input.attrs {
        if attr.path().is_ident("repr") {
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("C") {
                    has_repr_c = true;
                }
                Ok(())
            });
        }
    }
    if !has_repr_c {
        return Err(syn::Error::new_spanned(
            input,
            "Requires struct to have attribute #[repr(C)]",
        ));
    }
    Ok(())
}
fn get_encase_path() -> Path {
    match crate_name("graphics") {
        Ok(FoundCrate::Name(name)) => {
            let ident = syn::Ident::new(&name, proc_macro2::Span::call_site());
            parse_str(&format!("::{}::rendering::buffer_container", ident)).unwrap()
        }
        _ => parse_str("::graphics::rendering::buffer_container").unwrap(),
    }
}
#[proc_macro_attribute]
pub fn storage_data(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    let name = &input.ident;
    let crate_root = get_crate_root();

    let expanded = quote! {

        #[repr(C)]
        #[derive(#crate_root::rendering::buffer_container::ShaderType)]
        #input

        unsafe impl #crate_root::rendering::buffer_container::StorageData for #name {}
    };

    expanded.into()
}
#[proc_macro_attribute]
pub fn uniform_data(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    let name = &input.ident;
    let crate_root = get_crate_root();
    let expanded = quote! {

        #[repr(C)]
        #[derive(#crate_root::rendering::buffer_container::ShaderType)]
        #input

        unsafe impl #crate_root::rendering::buffer_container::UniformData for #name {}
    };

    expanded.into()
}

#[proc_macro_derive(VertexData)]
pub fn vertex_data_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    if let Err(e) = check_repr_c(&input) {
        return e.to_compile_error().into();
    }
    let crate_root = get_crate_root();

    let fields = match input.data {
        Data::Struct(s) => match s.fields {
            Fields::Named(f) => f.named,
            _ => panic!("Struct must not be an tuple or empty!"),
        },
        _ => panic!("VertexData works only with structs"),
    };

    let mut attrs = Vec::new();

    for (i, field) in fields.iter().enumerate() {
        let field_name = &field.ident;
        let field_type = &field.ty;
        let location = i;

        attrs.push(quote! { <#field_type as #crate_root::rendering::buffer_container::ToVertexAttribute>::to_attrib(core::mem::offset_of!(#name, #field_name),#location)});
    }

    let expanded = quote! {
        unsafe impl #crate_root::rendering::buffer_container::VertexData for #name {
            fn layout_info() -> Vec<Vec<#crate_root::rendering::buffer_container::VertexAttribute>> {
                vec![ #(#attrs),* ]
            }
        }
    };

    expanded.into()
}
implement!(get_encase_path());
