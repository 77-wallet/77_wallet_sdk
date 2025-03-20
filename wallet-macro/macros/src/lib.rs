mod macros;
extern crate proc_macro;

#[proc_macro_derive(Resource, attributes(resource))]
pub fn derive_entity(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    crate::macros::entity::expand_derive_entity(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
