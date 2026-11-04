//! Procedural macros for leafy.

// (`!(a && b)` often reads better than its expansion.)
#![allow(clippy::nonminimal_bool)]

mod css;
mod html;

/// Rewrites `::leafy::` paths to the name the calling crate imports leafy
/// under (e.g. `yew = { package = "leafy", .. }`).
fn fix_paths(ts: TokenStream2) -> TokenStream2 {
    use proc_macro_crate::{FoundCrate, crate_name};
    // Directly (maybe renamed), or through a crate named `yew` that
    // re-exports it.
    let name = match crate_name("leafy") {
        Ok(FoundCrate::Name(n)) => n,
        Ok(FoundCrate::Itself) => return ts,
        Err(_) => match crate_name("yew") {
            Ok(FoundCrate::Name(n)) => n,
            _ => return ts,
        },
    };
    if name == "leafy" {
        return ts;
    }
    let ident = Ident::new(&name, Span::call_site());
    rewrite(ts, &ident)
}

fn rewrite(ts: TokenStream2, to: &Ident) -> TokenStream2 {
    use proc_macro2::{Group, TokenTree};
    let mut out = Vec::new();
    let mut colons = 0;
    for tt in ts {
        let tt = match tt {
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
            TokenTree::Ident(i) if colons >= 2 && i == "leafy" => {
                colons = 0;
                TokenTree::Ident(Ident::new(&to.to_string(), i.span()))
            }
            tt => {
                colons = 0;
                tt
            }
        };
        out.push(tt);
    }
    out.into_iter().collect()
}

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{ToTokens, format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{
    Data, DeriveInput, Expr, Fields, FnArg, GenericParam, Ident, ItemFn, Token, Type,
    parse_macro_input,
};

/// Compiles yew-style markup into `Html`.
#[proc_macro]
pub fn html(input: TokenStream) -> TokenStream {
    let root = parse_macro_input!(input as html::HtmlRoot);
    fix_paths(html::gen_root(&root.0, false)).into()
}

/// As [`html!`], but a single component `<C .. />` is a
/// `VChild<C>` (for `ChildrenWithProps` and the like).
#[proc_macro]
pub fn html_nested(input: TokenStream) -> TokenStream {
    let root = parse_macro_input!(input as html::HtmlRoot);
    fix_paths(html::gen_root(&root.0, true)).into()
}

struct ClassesInput(Punctuated<Expr, Token![,]>);

impl Parse for ClassesInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(Self(Punctuated::parse_terminated(input)?))
    }
}

/// Builds `Classes` from things convertible into them.
#[proc_macro]
pub fn classes(input: TokenStream) -> TokenStream {
    let ClassesInput(exprs) = parse_macro_input!(input as ClassesInput);
    if exprs.len() == 1 {
        let e = &exprs[0];
        return fix_paths(quote!(<::leafy::Classes as ::std::convert::From<_>>::from(#e))).into();
    }
    let exprs = exprs.iter();
    fix_paths(quote!({
        #[allow(unused_mut)]
        let mut __c = ::leafy::Classes::new();
        #(__c.push(#exprs);)*
        __c
    }))
    .into()
}

/// A class for a stylesheet, written at compile time from a CSS literal
/// (as stylist's `css!`: declarations for the class, `&` for it in nested
/// rules, `@media` blocks). Evaluates to a `CssClass`, usable as (or in)
/// `Classes`, which puts the stylesheet on the page the first time it's
/// used.
#[proc_macro]
pub fn css(input: TokenStream) -> TokenStream {
    fix_paths(css::css(input.into(), false)).into()
}

/// A page-wide stylesheet, written at compile time from a CSS literal:
/// a `&'static Css`; its `register()` puts it on the page.
#[proc_macro]
pub fn global_css(input: TokenStream) -> TokenStream {
    fix_paths(css::css(input.into(), true)).into()
}

/// Marks a hook. (Does nothing; hooks are ordinary functions.)
#[proc_macro_attribute]
pub fn hook(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// Makes a component from a function of its properties.
#[proc_macro_attribute]
pub fn function_component(attr: TokenStream, item: TokenStream) -> TokenStream {
    let func = parse_macro_input!(item as ItemFn);
    let name: Option<Ident> = if attr.is_empty() {
        None
    } else {
        Some(parse_macro_input!(attr as Ident))
    };
    let name = name.unwrap_or_else(|| func.sig.ident.clone());
    let fn_name = &func.sig.ident;
    let vis = &func.vis;
    let docs = func.attrs.iter().filter(|a| a.path().is_ident("doc"));
    let generics = &func.sig.generics;
    // As yew: every type parameter of a component is `'static`.
    let mut impl_where = generics
        .where_clause
        .clone()
        .unwrap_or_else(|| syn::parse_quote!(where));
    for p in &generics.params {
        if let GenericParam::Type(t) = p {
            let ident = &t.ident;
            impl_where
                .predicates
                .push(syn::parse_quote!(#ident: 'static));
        }
    }
    let (impl_generics, ty_generics, _) = generics.split_for_impl();
    let where_clause = Some(&impl_where);
    let turbofish = ty_generics.as_turbofish();
    let props_ty: Type = match func.sig.inputs.first() {
        Some(FnArg::Typed(pt)) => match &*pt.ty {
            Type::Reference(r) => (*r.elem).clone(),
            _ => {
                return syn::Error::new_spanned(&pt.ty, "expected `&Props`")
                    .to_compile_error()
                    .into();
            }
        },
        Some(FnArg::Receiver(r)) => {
            return syn::Error::new_spanned(r, "unexpected receiver")
                .to_compile_error()
                .into();
        }
        None => syn::parse_quote!(()),
    };
    let call = if func.sig.inputs.is_empty() {
        quote!(#fn_name #turbofish())
    } else {
        quote!(#fn_name #turbofish(props))
    };
    let type_params: Vec<_> = generics
        .params
        .iter()
        .filter_map(|p| match p {
            GenericParam::Type(t) => Some(&t.ident),
            _ => None,
        })
        .collect();
    let decl_generics = {
        // (Without bounds, which are on the impl; with defaults.)
        let params = generics.params.iter().map(|p| match p {
            GenericParam::Type(t) => {
                let ident = &t.ident;
                match &t.default {
                    Some(d) => quote!(#ident = #d),
                    None => quote!(#ident),
                }
            }
            p => quote!(#p),
        });
        if generics.params.is_empty() {
            quote!()
        } else {
            quote!(<#(#params),*>)
        }
    };
    let mut func = func.clone();
    for p in func.sig.generics.params.iter_mut() {
        match p {
            GenericParam::Type(t) => {
                t.eq_token = None;
                t.default = None;
            }
            GenericParam::Const(c) => {
                c.eq_token = None;
                c.default = None;
            }
            GenericParam::Lifetime(_) => {}
        }
    }
    let fn_name_same = *fn_name == name;
    let allow = fn_name_same.then(|| quote!(#[allow(non_snake_case)]));
    fix_paths(quote!(
        #(#docs)*
        #[allow(non_camel_case_types)]
        #vis struct #name #decl_generics {
            _marker: ::std::marker::PhantomData<fn() -> (#(#type_params,)*)>,
        }

        impl #impl_generics ::leafy::BaseComponent for #name #ty_generics #where_clause {
            type Properties = #props_ty;
            #[inline]
            fn __run(
                props: &Self::Properties,
                _: &::std::rc::Rc<::leafy::__CompState>,
            ) -> ::std::option::Option<::leafy::Html> {
                let _ = props;
                ::std::option::Option::Some(#call)
            }
        }

        #allow
        #func
    ))
    .into()
}

/// Derives `Properties`: a builder that `html!` uses to set properties by
/// name, checking at compile time that required ones are set.
#[proc_macro_derive(Properties, attributes(prop_or, prop_or_default, prop_or_else))]
pub fn derive_properties(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match derive_properties_impl(input) {
        Ok(t) => fix_paths(t).into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Replaces `Self` in `ts` with `with`.
fn replace_self(ts: TokenStream2, with: &TokenStream2) -> TokenStream2 {
    use proc_macro2::{Group, TokenTree};
    ts.into_iter()
        .flat_map(|tt| -> Vec<TokenTree> {
            match tt {
                TokenTree::Ident(i) if i == "Self" => with.clone().into_iter().collect(),
                TokenTree::Group(g) => {
                    let mut ng = Group::new(g.delimiter(), replace_self(g.stream(), with));
                    ng.set_span(g.span());
                    vec![TokenTree::Group(ng)]
                }
                tt => vec![tt],
            }
        })
        .collect()
}

enum Default_ {
    Required,
    Default,
    Or(Expr),
    OrElse(Expr),
}

fn derive_properties_impl(input: DeriveInput) -> syn::Result<TokenStream2> {
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(&input, "expected a struct"));
    };
    let empty = Punctuated::new();
    let (fields, unit) = match &data.fields {
        Fields::Named(f) => (&f.named, false),
        Fields::Unit => (&empty, true),
        Fields::Unnamed(_) => {
            return Err(syn::Error::new_spanned(&input, "expected named fields"));
        }
    };
    let name = &input.ident;
    let vis = &input.vis;
    let builder = format_ident!("__{}Builder", name);
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // `Self` in a field's type means the properties, not the builder.
    let self_ty = quote!(#name #ty_generics);
    let mut fields_info = Vec::new();
    for f in fields {
        let mut default = Default_::Required;
        for a in &f.attrs {
            if a.path().is_ident("prop_or_default") {
                default = Default_::Default;
            } else if a.path().is_ident("prop_or") {
                default = Default_::Or(a.parse_args()?);
            } else if a.path().is_ident("prop_or_else") {
                default = Default_::OrElse(a.parse_args()?);
            }
        }
        let ty: Type = syn::parse2(replace_self(f.ty.to_token_stream(), &self_ty))?;
        fields_info.push((f.ident.clone().unwrap(), ty, default));
    }
    let required: Vec<usize> = fields_info
        .iter()
        .enumerate()
        .filter(|(_, f)| matches!(f.2, Default_::Required))
        .map(|(i, _)| i)
        .collect();
    let markers: Vec<Ident> = (0..required.len())
        .map(|i| Ident::new(&format!("__R{i}"), Span::call_site()))
        .collect();

    // Builder generics: the props' params (with bounds, without defaults),
    // then markers.
    let params: Vec<GenericParam> = generics
        .params
        .iter()
        .cloned()
        .map(|mut p| {
            match &mut p {
                GenericParam::Type(t) => {
                    t.eq_token = None;
                    t.default = None;
                }
                GenericParam::Const(c) => {
                    c.eq_token = None;
                    c.default = None;
                }
                GenericParam::Lifetime(_) => {}
            }
            p
        })
        .collect();
    let param_names: Vec<TokenStream2> = generics
        .params
        .iter()
        .map(|p| match p {
            GenericParam::Type(t) => {
                let i = &t.ident;
                quote!(#i)
            }
            GenericParam::Lifetime(l) => {
                let l = &l.lifetime;
                quote!(#l)
            }
            GenericParam::Const(c) => {
                let i = &c.ident;
                quote!(#i)
            }
        })
        .collect();
    let type_param_names: Vec<&Ident> = generics
        .params
        .iter()
        .filter_map(|p| match p {
            GenericParam::Type(t) => Some(&t.ident),
            _ => None,
        })
        .collect();
    let field_names: Vec<&Ident> = fields_info.iter().map(|f| &f.0).collect();
    let field_tys: Vec<&Type> = fields_info.iter().map(|f| &f.1).collect();
    let unit_markers = markers.iter().map(|_| quote!(()));

    let setters = fields_info.iter().enumerate().map(|(i, (fname, fty, _))| {
        if let Some(r) = required.iter().position(|&j| j == i) {
            let out_markers = markers.iter().enumerate().map(|(k, m)| {
                if k == r {
                    quote!(::leafy::__Set)
                } else {
                    quote!(#m)
                }
            });
            let others = field_names.iter().filter(|n| **n != fname).collect::<Vec<_>>();
            quote!(
                #[inline(always)]
                #[allow(non_snake_case)]
                pub fn #fname(self, value: impl ::leafy::IntoPropValue<#fty>) -> #builder<#(#param_names,)* #(#out_markers),*> {
                    #builder {
                        #fname: ::std::option::Option::Some(value.into_prop_value()),
                        #(#others: self.#others,)*
                        __marker: ::std::marker::PhantomData,
                    }
                }
            )
        } else {
            quote!(
                #[inline(always)]
                #[allow(non_snake_case)]
                pub fn #fname(mut self, value: impl ::leafy::IntoPropValue<#fty>) -> Self {
                    self.#fname = ::std::option::Option::Some(value.into_prop_value());
                    self
                }
            )
        }
    });

    let build_fields = fields_info.iter().map(|(fname, fty, default)| match default {
        Default_::Required => {
            quote!(#fname: match self.#fname { ::std::option::Option::Some(v) => v, ::std::option::Option::None => ::leafy::__missing_prop() })
        }
        Default_::Default => {
            quote!(#fname: self.#fname.unwrap_or_default())
        }
        Default_::Or(e) => {
            quote!(#fname: match self.#fname { ::std::option::Option::Some(v) => v, ::std::option::Option::None => { let v: #fty = #e; v } })
        }
        Default_::OrElse(e) => {
            quote!(#fname: match self.#fname { ::std::option::Option::Some(v) => v, ::std::option::Option::None => { let v: #fty = (#e)(); v } })
        }
    });

    let construct = if unit {
        let _ = build_fields;
        quote!(#name)
    } else {
        quote!(#name { #(#build_fields,)* })
    };

    let where_preds = where_clause.map(|w| {
        let p = w.predicates.iter();
        quote!(#(#p,)*)
    });

    // Shared by every use of the component (there is one instantiation, as
    // all required properties are set), unless trivial.
    let defaulted = fields_info.len() - required.len();
    let build_inline = if defaulted > 0 {
        quote!(#[inline(never)])
    } else {
        quote!(#[inline(always)])
    };
    // (Out of line, `builder()` costs more than it saves.)
    let builder_inline = quote!(#[inline(always)]);
    Ok(quote!(
        #[doc(hidden)]
        #[allow(non_camel_case_types, non_snake_case)]
        #vis struct #builder<#(#params,)* #(#markers),*> #where_clause {
            #(#field_names: ::std::option::Option<#field_tys>,)*
            __marker: ::std::marker::PhantomData<(fn() -> (#(#type_param_names,)*), #(#markers,)*)>,
        }

        impl #impl_generics ::leafy::Properties for #name #ty_generics #where_clause {
            type Builder = #builder<#(#param_names,)* #(#unit_markers),*>;
            #builder_inline
            fn builder() -> Self::Builder {
                #builder {
                    #(#field_names: ::std::option::Option::None,)*
                    __marker: ::std::marker::PhantomData,
                }
            }
        }

        #[allow(non_camel_case_types)]
        impl<#(#params,)* #(#markers),*> #builder<#(#param_names,)* #(#markers),*> #where_clause {
            #(#setters)*
        }

        #[allow(non_camel_case_types)]
        impl<#(#params,)* #(#markers),*> ::leafy::__Build for #builder<#(#param_names,)* #(#markers),*>
        where
            #where_preds
            #(#markers: ::leafy::__IsSet,)*
        {
            type Output = #name #ty_generics;
            #[inline(always)]
            fn __build(self) -> #name #ty_generics {
                #construct
            }
        }

        #[allow(non_camel_case_types)]
        impl<#(#params,)* #(#markers),*> #builder<#(#param_names,)* #(#markers),*>
        where
            #where_preds
            #(#markers: ::leafy::__IsSet,)*
        {
            #build_inline
            pub fn build(self) -> #name #ty_generics {
                ::leafy::__Build::__build(self)
            }
        }
    ))
}

/// `props!(Type { a: 1, b })`: a `Type` (a `Properties`), built as `html!`
/// builds a component's: values converted with `IntoPropValue`, a missing
/// required one a compile error, the rest their defaults. `Type` may be a
/// component's `Comp::Properties`.
///
/// ```compile_fail
/// # use leafy::prelude::*;
/// #[derive(Properties, PartialEq)]
/// struct P { id: u32 }
/// let _ = leafy::props!(P {});
/// ```
#[proc_macro]
pub fn props(input: TokenStream) -> TokenStream {
    let PropsInput { ty, fields } = parse_macro_input!(input as PropsInput);
    let sets = fields.iter().map(|(name, value)| {
        let name = html::prop_ident(name);
        quote::quote_spanned!(name.span()=> .#name(#value))
    });
    fix_paths(quote!({
        ::leafy::__Build::__build(<#ty as ::leafy::Properties>::builder() #(#sets)*)
    }))
    .into()
}

struct PropsInput {
    ty: syn::Type,
    fields: Vec<(Ident, syn::Expr)>,
}

impl Parse for PropsInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut path: syn::TypePath = input.parse()?;
        // `Comp::Properties`, as `<Comp as BaseComponent>::Properties`.
        let ty = if path.qself.is_none()
            && path.path.segments.len() > 1
            && path
                .path
                .segments
                .last()
                .is_some_and(|s| s.ident == "Properties")
        {
            path.path.segments.pop();
            path.path.segments.pop_punct();
            syn::parse_quote!(<#path as ::leafy::BaseComponent>::Properties)
        } else {
            syn::Type::Path(path)
        };
        let content;
        syn::braced!(content in input);
        let mut fields = Vec::new();
        while !content.is_empty() {
            let name: Ident = syn::ext::IdentExt::parse_any(&content)?;
            let value = if content.peek(syn::Token![:]) {
                content.parse::<syn::Token![:]>()?;
                content.parse()?
            } else {
                syn::parse_quote!(#name)
            };
            fields.push((name, value));
            if content.is_empty() {
                break;
            }
            content.parse::<syn::Token![,]>()?;
        }
        if !input.is_empty() {
            return Err(input.error("unexpected tokens after the properties"));
        }
        Ok(Self { ty, fields })
    }
}
