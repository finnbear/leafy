//! The `Routable` derive for leafy_router: a port of yew-router-macro 0.20.

mod routable_derive;
use proc_macro2::{Group, Ident, Span, TokenStream, TokenTree};
use routable_derive::{routable_derive_impl, Routable};
use syn::parse_macro_input;

/// Rewrites `::leafy_router::` paths to the name the calling crate imports
/// the router under (e.g. `yew-router = { package = "leafy_router", .. }`),
/// or through a crate named `yew-router` that re-exports it.
fn fix_paths(ts: TokenStream) -> TokenStream {
    use proc_macro_crate::{crate_name, FoundCrate};
    let name = match crate_name("leafy_router") {
        Ok(FoundCrate::Name(n)) => n,
        Ok(FoundCrate::Itself) => return ts,
        Err(_) => match crate_name("yew-router") {
            Ok(FoundCrate::Name(n)) => n,
            _ => return ts,
        },
    };
    if name == "leafy_router" {
        return ts;
    }
    rewrite(ts, &Ident::new(&name, Span::call_site()))
}

fn rewrite(ts: TokenStream, to: &Ident) -> TokenStream {
    let mut colons = 0;
    ts.into_iter()
        .map(|tt| match tt {
            TokenTree::Group(g) => {
                let mut ng = Group::new(g.delimiter(), rewrite(g.stream(), to));
                ng.set_span(g.span());
                colons = 0;
                TokenTree::Group(ng)
            }
            TokenTree::Punct(p) => {
                colons = if p.as_char() == ':' { colons + 1 } else { 0 };
                TokenTree::Punct(p)
            }
            TokenTree::Ident(i) if colons >= 2 && i == "leafy_router" => {
                colons = 0;
                TokenTree::Ident(Ident::new(&to.to_string(), i.span()))
            }
            tt => {
                colons = 0;
                tt
            }
        })
        .collect()
}

/// Derive macro used to mark an enum as Routable.
///
/// This macro can only be used on enums. Every variant of the macro needs to be marked
/// with the `at` attribute to specify the URL of the route. It generates an implementation of
///  `leafy_router::Routable` trait and `const`s for the routes passed which are used with `Route`
/// component.
///
/// # Example
///
/// ```ignore
/// # use leafy_router::Routable;
/// #[derive(Debug, Clone, Copy, PartialEq, Routable)]
/// enum Routes {
///     #[at("/")]
///     Home,
///     #[at("/secure")]
///     Secure,
///     #[at("/404")]
///     NotFound,
/// }
/// ```
#[proc_macro_derive(Routable, attributes(at, not_found))]
pub fn routable_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as Routable);
    fix_paths(routable_derive_impl(input)).into()
}
