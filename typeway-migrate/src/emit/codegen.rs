//! Format token streams into pretty-printed Rust source code.

use proc_macro2::TokenStream;

/// Format a token stream into a pretty-printed Rust source string.
pub fn format_tokens(tokens: &TokenStream) -> String {
    let file: syn::File = match syn::parse2(tokens.clone()) {
        Ok(f) => f,
        Err(_) => {
            // If the token stream isn't a valid file, return it unformatted.
            return tokens.to_string();
        }
    };
    prettyplease::unparse(&file)
}
