extern crate proc_macro;
use proc_macro::TokenStream;

#[proc_macro]
pub fn magic(input: TokenStream) -> TokenStream {
    input
}
