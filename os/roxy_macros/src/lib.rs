use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn init_data(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // TODO: Put the item into a linker section that can be placed at the end of the binary and freed after initialization.
    item
}
