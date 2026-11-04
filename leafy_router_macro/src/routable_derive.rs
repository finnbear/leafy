use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Fields, Ident, LitStr, Variant};

const AT_ATTR_IDENT: &str = "at";
const NOT_FOUND_ATTR_IDENT: &str = "not_found";

pub struct Routable {
    ident: Ident,
    ats: Vec<LitStr>,
    variants: Punctuated<Variant, syn::token::Comma>,
    not_found_route: Option<Ident>,
}

impl Parse for Routable {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let DeriveInput { ident, data, .. } = input.parse()?;

        let data = match data {
            Data::Enum(data) => data,
            Data::Struct(s) => {
                return Err(syn::Error::new(
                    s.struct_token.span(),
                    "expected enum, found struct",
                ))
            }
            Data::Union(u) => {
                return Err(syn::Error::new(
                    u.union_token.span(),
                    "expected enum, found union",
                ))
            }
        };

        let (not_found_route, ats) = parse_variants_attributes(&data.variants)?;

        Ok(Self {
            ident,
            variants: data.variants,
            ats,
            not_found_route,
        })
    }
}

fn parse_variants_attributes(
    variants: &Punctuated<Variant, syn::token::Comma>,
) -> syn::Result<(Option<Ident>, Vec<LitStr>)> {
    let mut not_founds = vec![];
    let mut ats: Vec<LitStr> = vec![];

    let mut not_found_attrs = vec![];

    for variant in variants.iter() {
        if let Fields::Unnamed(ref field) = variant.fields {
            return Err(syn::Error::new(
                field.span(),
                "only named fields are supported",
            ));
        }

        let attrs = &variant.attrs;
        let at_attrs = attrs
            .iter()
            .filter(|attr| attr.path().is_ident(AT_ATTR_IDENT))
            .collect::<Vec<_>>();

        let attr = match at_attrs.len() {
            1 => *at_attrs.first().unwrap(),
            0 => {
                return Err(syn::Error::new(
                    variant.span(),
                    format!("{AT_ATTR_IDENT} attribute must be present on every variant"),
                ))
            }
            _ => {
                return Err(syn::Error::new_spanned(
                    quote! { #(#at_attrs)* },
                    format!("only one {AT_ATTR_IDENT} attribute must be present"),
                ))
            }
        };

        let lit = attr.parse_args::<LitStr>()?;
        let val = lit.value();

        if val.find('#').is_some() {
            return Err(syn::Error::new_spanned(
                lit,
                "You cannot use `#` in your routes. Please consider `HashRouter` instead.",
            ));
        }

        if !val.starts_with('/') {
            return Err(syn::Error::new_spanned(
                lit,
                "relative paths are not supported at this moment.",
            ));
        }

        ats.push(lit);

        for attr in attrs.iter() {
            if attr.path().is_ident(NOT_FOUND_ATTR_IDENT) {
                not_found_attrs.push(attr);
                not_founds.push(variant.ident.clone())
            }
        }
    }

    if not_founds.len() > 1 {
        return Err(syn::Error::new_spanned(
            quote! { #(#not_found_attrs)* },
            format!("there can only be one {NOT_FOUND_ATTR_IDENT}"),
        ));
    }

    Ok((not_founds.into_iter().next(), ats))
}

impl Routable {
    fn build_from_path(&self) -> TokenStream {
        let from_path_matches = self.variants.iter().enumerate().map(|(i, variant)| {
            let ident = &variant.ident;
            let right = match &variant.fields {
                Fields::Unit => quote! { Self::#ident },
                Fields::Named(field) => {
                    let fields = field.named.iter().map(|it| {
                        // named fields have idents
                        it.ident.as_ref().unwrap()
                    });
                    quote! { Self::#ident { #(#fields: {
                        let param = params.get(stringify!(#fields))?;
                        let param = &*::leafy_router::__macro::decode_for_url(param).ok()?;
                        let param = param.parse().ok()?;
                        param
                    },)* } }
                }
                Fields::Unnamed(_) => unreachable!(), // already checked
            };

            let left = self.ats.get(i).unwrap();
            quote! {
                #left => ::std::option::Option::Some(#right)
            }
        });

        quote! {
            fn from_path(path: &str, params: &::std::collections::HashMap<&str, &str>) -> ::std::option::Option<Self> {
                match path {
                    #(#from_path_matches),*,
                    _ => ::std::option::Option::None,
                }
            }
        }
    }

    fn build_to_path(&self) -> TokenStream {
        let to_path_matches = self.variants.iter().enumerate().map(|(i, variant)| {
            let ident = &variant.ident;
            let mut right = self.ats.get(i).unwrap().value();

            match &variant.fields {
                Fields::Unit => quote! { Self::#ident => ::std::string::ToString::to_string(#right) },
                Fields::Named(field) => {
                    let fields = field
                        .named
                        .iter()
                        .map(|it| it.ident.as_ref().unwrap())
                        .collect::<Vec<_>>();

                    let mut wildcard_fields = std::collections::HashSet::new();
                    for field in fields.iter() {
                        if right.contains(&format!("*{field}")) {
                            wildcard_fields.insert((*field).clone());
                        }
                        // :param -> {param}
                        // *param -> {param}
                        // so we can pass it to `format!("...", param)`
                        right = right.replace(&format!(":{field}"), &format!("{{{field}}}"));
                        right = right.replace(&format!("*{field}"), &format!("{{{field}}}"));
                    }

                    let field_encodings = fields.iter().map(|field| {
                        if wildcard_fields.contains(*field) {
                            quote! {
                                #field = ::leafy_router::__macro::encode_path_for_url(&::std::format!("{}", #field))
                            }
                        } else {
                            quote! {
                                #field = ::leafy_router::__macro::encode_for_url(&::std::format!("{}", #field))
                            }
                        }
                    });

                    quote! {
                        Self::#ident { #(#fields),* } => ::std::format!(#right, #(#field_encodings),*)
                    }
                }
                Fields::Unnamed(_) => unreachable!(), // already checked
            }
        });

        quote! {
            fn to_path(&self) -> ::std::string::String {
                match self {
                    #(#to_path_matches),*,
                }
            }
        }
    }
}

/// A route compiled for matching: its parts, and the field each capture
/// goes to (`None` for an unnamed wildcard).
struct Compiled {
    parts: Vec<TokenStream>,
    captures: Vec<Option<String>>,
    /// As `route-recognizer` ranks routes: static segments, `:params`,
    /// `*wildcards`.
    rank: (u32, u32, u32),
}

/// Compiles a route as `route-recognizer` reads it: without its leading
/// `/` and any trailing `/`, split into segments after each `/` or `.`.
fn compile(route: &str) -> Compiled {
    let route = route.strip_prefix('/').unwrap_or(route);
    let route = route.strip_suffix('/').unwrap_or(route);
    let mut parts = Vec::new();
    let mut lit = String::new();
    let mut captures = Vec::new();
    let mut rank = (0, 0, 0);
    let is_sep = |c: char| c == '/' || c == '.';
    let mut rest = route;
    while !rest.is_empty() {
        // A separator, then a segment up to the next one.
        let (sep, seg) = match rest.chars().next() {
            Some(c) if is_sep(c) => (Some(c), &rest[1..]),
            _ => (None, rest),
        };
        let end = seg
            .char_indices()
            .skip(if sep.is_some() { 0 } else { 1 })
            .find(|&(_, c)| is_sep(c))
            .map_or(seg.len(), |(i, _)| i);
        let (seg, next) = seg.split_at(end);
        rest = next;
        if let Some(sep) = sep {
            lit.push(sep);
        }
        let capture = if let Some(name) = seg.strip_prefix(':') {
            rank.1 += 1;
            Some((quote!(::leafy_router::__macro::Part::Param), name))
        } else if let Some(name) = seg.strip_prefix('*') {
            rank.2 += 1;
            Some((quote!(::leafy_router::__macro::Part::Wild), name))
        } else {
            rank.0 += 1;
            lit.push_str(seg);
            None
        };
        if let Some((part, name)) = capture {
            if !lit.is_empty() {
                parts.push(quote!(::leafy_router::__macro::Part::Lit(#lit)));
                lit.clear();
            }
            parts.push(part);
            captures.push((!name.is_empty()).then(|| name.to_string()));
        }
    }
    if !lit.is_empty() {
        parts.push(quote!(::leafy_router::__macro::Part::Lit(#lit)));
    }
    Compiled {
        parts,
        captures,
        rank,
    }
}

impl Routable {
    /// `recognize`: the routes, compiled and best first, matched by one
    /// shared function; then the matching variant, with its fields parsed.
    fn build_recognize(&self) -> syn::Result<TokenStream> {
        let mut routes: Vec<(usize, Compiled)> = self
            .ats
            .iter()
            .enumerate()
            .map(|(i, at)| (i, compile(&at.value())))
            .collect();
        // Best first; stable, so the first declared of equals first.
        routes.sort_by_key(|r| std::cmp::Reverse(r.1.rank));
        let n_caps = routes.iter().map(|r| r.1.captures.len()).max().unwrap_or(0);
        let tables = routes.iter().map(|(_, c)| {
            let parts = &c.parts;
            quote!(&[#(#parts),*])
        });
        let mut arms = Vec::new();
        for (index, (i, c)) in routes.iter().enumerate() {
            let variant = self.variants.iter().nth(*i).unwrap();
            let ident = &variant.ident;
            let make = match &variant.fields {
                Fields::Unit => quote!(::std::option::Option::Some(Self::#ident)),
                Fields::Named(fields) => {
                    let mut inits = Vec::new();
                    for f in &fields.named {
                        let name = f.ident.as_ref().unwrap();
                        let Some(k) = c
                            .captures
                            .iter()
                            .position(|c| c.as_deref() == Some(&*name.to_string()))
                        else {
                            return Err(syn::Error::new_spanned(
                                name,
                                format!("`{name}` is not in the route `{}`", self.ats[*i].value()),
                            ));
                        };
                        inits.push(quote!(#name: ::leafy_router::__macro::param(__c[#k])?));
                    }
                    quote!((|| ::std::option::Option::Some(Self::#ident { #(#inits),* }))())
                }
                Fields::Unnamed(_) => unreachable!(),
            };
            arms.push(quote!(::std::option::Option::Some(#index) => #make,));
        }
        Ok(quote! {
            fn recognize(pathname: &str) -> ::std::option::Option<Self> {
                static ROUTES: &[&[::leafy_router::__macro::Part]] = &[#(#tables),*];
                let mut __c = [""; #n_caps];
                let route = match ::leafy_router::__macro::recognize(ROUTES, pathname, &mut __c) {
                    #(#arms)*
                    _ => ::std::option::Option::None,
                };
                route.or_else(Self::not_found_route)
            }
        })
    }
}

pub fn routable_derive_impl(input: Routable) -> TokenStream {
    let Routable {
        ats,
        not_found_route,
        ident,
        ..
    } = &input;

    let from_path = input.build_from_path();
    let to_path = input.build_to_path();
    let recognize = match input.build_recognize() {
        Ok(r) => r,
        Err(e) => return e.to_compile_error(),
    };

    let maybe_not_found_route = match not_found_route {
        Some(route) => quote! { ::std::option::Option::Some(Self::#route) },
        None => quote! { ::std::option::Option::None },
    };

    let maybe_default = match not_found_route {
        Some(route) => {
            quote! {
                impl ::std::default::Default for #ident {
                    fn default() -> Self {
                        Self::#route
                    }
                }
            }
        }
        None => TokenStream::new(),
    };

    quote! {
        #[automatically_derived]
        impl ::leafy_router::Routable for #ident {
            #from_path
            #to_path

            type Routes = ::std::iter::Copied<::std::slice::Iter<'static, &'static str>>;

            fn routes() -> Self::Routes {
                const ROUTES: &[&str] = &[#(#ats),*];
                ROUTES.iter().copied()
            }

            fn not_found_route() -> ::std::option::Option<Self> {
                #maybe_not_found_route
            }

            #recognize
        }

        #maybe_default
    }
}
