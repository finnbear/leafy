//! `html!`: parses yew's syntax and compiles element trees into static
//! templates (an HTML string and a walk program, see the runtime's `dom.rs`)
//! plus a vector of hole values.

use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote, quote_spanned};
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{Expr, Ident, Lit, Pat, Token, TypePath, braced};

// ------------------------------------------------------------------ parsing

pub enum Node {
    Element(Element),
    Component(Component),
    Fragment(Vec<Node>, Option<TokenStream>),
    Text(String),
    Block(TokenStream),
    Iter(TokenStream),
    If(If),
    For(For),
}

pub struct Element {
    tag: String,
    span: Span,
    attrs: Vec<Attr>,
    children: Vec<Node>,
}

pub struct Attr {
    name: String,
    span: Span,
    value: AttrVal,
}

pub enum AttrVal {
    Lit(Lit),
    Expr(TokenStream),
}

pub struct Component {
    path: TypePath,
    props: Vec<(Ident, AttrVal)>,
    base: Option<Expr>,
    key: Option<TokenStream>,
    children: Option<Vec<Node>>,
}

pub struct If {
    cond: Expr,
    then: Vec<Node>,
    els: Option<Box<Node>>,
}

pub struct For {
    pat: Pat,
    expr: Expr,
    body: Vec<Node>,
}

pub struct HtmlRoot(pub Vec<Node>);

impl Parse for HtmlRoot {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut nodes = Vec::new();
        while !input.is_empty() {
            nodes.push(parse_node(input)?);
        }
        Ok(Self(nodes))
    }
}

fn parse_nodes(input: ParseStream) -> syn::Result<Vec<Node>> {
    let mut nodes = Vec::new();
    while !input.is_empty() && !(input.peek(Token![<]) && input.peek2(Token![/])) {
        nodes.push(parse_node(input)?);
    }
    Ok(nodes)
}

fn lit_text(lit: &Lit) -> Option<String> {
    Some(match lit {
        Lit::Str(s) => s.value(),
        Lit::Char(c) => c.value().to_string(),
        Lit::Int(i) => i.base10_digits().to_string(),
        Lit::Float(f) => f.base10_digits().to_string(),
        Lit::Bool(b) => b.value.to_string(),
        _ => return None,
    })
}

fn parse_node(input: ParseStream) -> syn::Result<Node> {
    if input.peek(Token![<]) {
        parse_tag(input)
    } else if input.peek(syn::token::Brace) {
        let content;
        braced!(content in input);
        if content.is_empty() {
            // `{}`, or `{ /* a comment */ }`.
            return Ok(Node::Fragment(Vec::new(), None));
        }
        if content.peek(Token![for]) {
            content.parse::<Token![for]>()?;
            return Ok(Node::Iter(content.parse()?));
        }
        let fork = content.fork();
        if let Ok(lit) = fork.parse::<Lit>()
            && fork.is_empty()
            && let Some(text) = lit_text(&lit)
        {
            content.parse::<TokenStream>()?;
            return Ok(Node::Text(text));
        }
        let tokens: TokenStream = content.parse()?;
        Ok(Node::Block(tokens))
    } else if input.peek(Token![if]) {
        Ok(Node::If(parse_if(input)?))
    } else if input.peek(Token![for]) {
        input.parse::<Token![for]>()?;
        let pat = Pat::parse_multi_with_leading_vert(input)?;
        input.parse::<Token![in]>()?;
        let expr = Expr::parse_without_eager_brace(input)?;
        let content;
        braced!(content in input);
        let body = parse_nodes(&content)?;
        Ok(Node::For(For { pat, expr, body }))
    } else if input.peek(Lit) {
        let lit: Lit = input.parse()?;
        lit_text(&lit)
            .map(Node::Text)
            .ok_or_else(|| syn::Error::new(lit.span(), "unsupported literal"))
    } else {
        Err(input.error("expected an element, component, `{expression}`, `if` or `for`"))
    }
}

fn parse_if(input: ParseStream) -> syn::Result<If> {
    input.parse::<Token![if]>()?;
    let cond = Expr::parse_without_eager_brace(input)?;
    let content;
    braced!(content in input);
    let then = parse_nodes(&content)?;
    let els = if input.peek(Token![else]) {
        input.parse::<Token![else]>()?;
        if input.peek(Token![if]) {
            Some(Box::new(Node::If(parse_if(input)?)))
        } else {
            let content;
            braced!(content in input);
            Some(Box::new(Node::Fragment(parse_nodes(&content)?, None)))
        }
    } else {
        None
    };
    Ok(If { cond, then, els })
}

/// An element name, possibly with dashes.
fn parse_dashed(input: ParseStream) -> syn::Result<(String, Span)> {
    let first = Ident::parse_any(input)?;
    let span = first.span();
    let mut name = first.to_string();
    while input.peek(Token![-]) {
        input.parse::<Token![-]>()?;
        let next = Ident::parse_any(input)?;
        name.push('-');
        name.push_str(&next.to_string());
    }
    Ok((name.trim_start_matches("r#").to_string(), span))
}

fn is_element_name(input: ParseStream) -> bool {
    let fork = input.fork();
    let Ok(ident) = Ident::parse_any(&fork) else {
        return false;
    };
    let s = ident.to_string();
    !fork.peek(Token![::]) && s.chars().next().is_some_and(|c| c.is_ascii_lowercase())
}

fn parse_attr_val(input: ParseStream) -> syn::Result<AttrVal> {
    if input.peek(syn::token::Brace) {
        let content;
        braced!(content in input);
        let fork = content.fork();
        if let Ok(lit) = fork.parse::<Lit>()
            && fork.is_empty()
        {
            content.parse::<TokenStream>()?;
            return Ok(AttrVal::Lit(lit));
        }
        let tokens: TokenStream = content.parse()?;
        Ok(AttrVal::Expr(block_expr(tokens)))
    } else if input.peek(Lit) {
        Ok(AttrVal::Lit(input.parse()?))
    } else if input.peek(Token![-]) && input.peek2(Lit) {
        // A negative number, e.g. `min=-5`.
        input.parse::<Token![-]>()?;
        let lit: Lit = input.parse()?;
        let span = lit.span();
        let lit = match lit {
            Lit::Int(i) => Lit::Int(syn::LitInt::new(&format!("-{i}"), span)),
            Lit::Float(f) => Lit::Float(syn::LitFloat::new(&format!("-{f}"), span)),
            l => return Err(syn::Error::new(l.span(), "expected a number")),
        };
        Ok(AttrVal::Lit(lit))
    } else {
        // An unbraced path.
        let e: syn::ExprPath = input.parse()?;
        Ok(AttrVal::Expr(e.to_token_stream()))
    }
}

/// Tokens up to (not including) `>` or `/>`.
fn parse_until_tag_end(input: ParseStream) -> syn::Result<TokenStream> {
    let mut out = TokenStream::new();
    while !input.is_empty()
        && !input.peek(Token![>])
        && !(input.peek(Token![/]) && input.peek2(Token![>]))
    {
        let tt: proc_macro2::TokenTree = input.parse()?;
        out.extend(std::iter::once(tt));
    }
    Ok(out)
}

const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Whether the next tokens are `</tag>`.
fn peek_close(input: ParseStream, tag: &str) -> bool {
    let fork = input.fork();
    fork.parse::<Token![<]>().is_ok()
        && fork.parse::<Token![/]>().is_ok()
        && parse_dashed(&fork).is_ok_and(|(name, _)| name == tag)
}

fn parse_tag(input: ParseStream) -> syn::Result<Node> {
    input.parse::<Token![<]>()?;
    if input.peek(Token![>]) {
        // Fragment.
        input.parse::<Token![>]>()?;
        let children = parse_nodes(input)?;
        input.parse::<Token![<]>()?;
        input.parse::<Token![/]>()?;
        input.parse::<Token![>]>()?;
        return Ok(Node::Fragment(children, None));
    }
    if input.peek(Token![/]) {
        return Err(input.error("unexpected closing tag"));
    }
    if input.peek(Ident)
        && input.peek2(Token![=])
        && input.fork().parse::<Ident>().is_ok_and(|i| i == "key")
    {
        // A keyed fragment: `<key={k}>..</>`.
        input.parse::<Ident>()?;
        input.parse::<Token![=]>()?;
        let key = match parse_attr_val(input)? {
            AttrVal::Lit(l) => l.to_token_stream(),
            AttrVal::Expr(e) => e,
        };
        input.parse::<Token![>]>()?;
        let children = parse_nodes(input)?;
        input.parse::<Token![<]>()?;
        input.parse::<Token![/]>()?;
        input.parse::<Token![>]>()?;
        return Ok(Node::Fragment(children, Some(key)));
    }
    if is_element_name(input) {
        let (tag, span) = parse_dashed(input)?;
        let mut attrs = Vec::new();
        loop {
            if input.peek(Token![/]) && input.peek2(Token![>]) {
                input.parse::<Token![/]>()?;
                input.parse::<Token![>]>()?;
                return Ok(Node::Element(Element {
                    tag,
                    span,
                    attrs,
                    children: Vec::new(),
                }));
            }
            if input.peek(Token![>]) {
                input.parse::<Token![>]>()?;
                break;
            }
            if input.peek(syn::token::Brace) {
                let content;
                braced!(content in input);
                let ident = Ident::parse_any(&content)?;
                attrs.push(Attr {
                    name: ident.to_string().trim_start_matches("r#").to_string(),
                    span: ident.span(),
                    value: AttrVal::Expr(ident.to_token_stream()),
                });
                continue;
            }
            let (name, aspan) = parse_dashed(input)?;
            input.parse::<Token![=]>()?;
            let value = parse_attr_val(input)?;
            attrs.push(Attr {
                name,
                span: aspan,
                value,
            });
        }
        // A void element needs no `/>`, but may have a closing tag.
        if VOID.contains(&tag.as_str()) && !peek_close(input, &tag) {
            return Ok(Node::Element(Element {
                tag,
                span,
                attrs,
                children: Vec::new(),
            }));
        }
        let children = parse_nodes(input)?;
        input.parse::<Token![<]>()?;
        input.parse::<Token![/]>()?;
        let (close, cspan) = parse_dashed(input)?;
        if close != tag {
            return Err(syn::Error::new(
                cspan,
                format!("expected `</{tag}>`, found `</{close}>`"),
            ));
        }
        input.parse::<Token![>]>()?;
        Ok(Node::Element(Element {
            tag,
            span,
            attrs,
            children,
        }))
    } else {
        let path: TypePath = input.parse()?;
        let mut props = Vec::new();
        let mut base = None;
        let mut key = None;
        let self_closing;
        loop {
            if input.peek(Token![/]) && input.peek2(Token![>]) {
                input.parse::<Token![/]>()?;
                input.parse::<Token![>]>()?;
                self_closing = true;
                break;
            }
            if input.peek(Token![>]) {
                input.parse::<Token![>]>()?;
                self_closing = false;
                break;
            }
            if input.peek(Token![..]) {
                input.parse::<Token![..]>()?;
                let tokens = parse_until_tag_end(input)?;
                base = Some(syn::parse2::<Expr>(tokens)?);
                continue;
            }
            if input.peek(syn::token::Brace) {
                let content;
                braced!(content in input);
                let ident = Ident::parse_any(&content)?;
                props.push((ident.clone(), AttrVal::Expr(ident.to_token_stream())));
                continue;
            }
            let ident = Ident::parse_any(input)?;
            input.parse::<Token![=]>()?;
            let value = parse_attr_val(input)?;
            if ident == "key" {
                key = Some(match value {
                    AttrVal::Lit(l) => l.to_token_stream(),
                    AttrVal::Expr(e) => e,
                });
            } else {
                props.push((ident, value));
            }
        }
        let children = if self_closing {
            None
        } else {
            let children = parse_nodes(input)?;
            input.parse::<Token![<]>()?;
            input.parse::<Token![/]>()?;
            let close: TypePath = input.parse()?;
            if close.to_token_stream().to_string() != path.to_token_stream().to_string() {
                return Err(syn::Error::new(close.span(), "mismatched closing tag"));
            }
            input.parse::<Token![>]>()?;
            Some(children)
        };
        Ok(Node::Component(Component {
            path,
            props,
            base,
            key,
            children,
        }))
    }
}

/// The contents of `{ .. }` as an expression: a lone expression as itself
/// (as yew does, so temporaries live as long as they would there), otherwise
/// as a block.
fn block_expr(tokens: TokenStream) -> TokenStream {
    let parsed = syn::parse::Parser::parse2(syn::Block::parse_within, tokens.clone());
    if let Ok(mut stmts) = parsed
        && stmts.len() == 1
    {
        match stmts.remove(0) {
            syn::Stmt::Expr(e, None) => return e.to_token_stream(),
            syn::Stmt::Macro(m) if m.semi_token.is_none() => return m.mac.to_token_stream(),
            _ => {}
        }
    }
    quote!({ #tokens })
}

// ------------------------------------------------------------------ codegen

/// The expression for a list of nodes.
pub fn gen_nodes(nodes: &[Node]) -> TokenStream {
    match nodes {
        [] => quote!(<::leafy::Html as ::std::default::Default>::default()),
        [node] => gen_node(node),
        nodes => gen_template(nodes, None),
    }
}

fn gen_node(node: &Node) -> TokenStream {
    match node {
        Node::Element(e) => {
            let key = e
                .attrs
                .iter()
                .find(|a| a.name == "key")
                .map(|a| match &a.value {
                    AttrVal::Lit(l) => l.to_token_stream(),
                    AttrVal::Expr(e) => e.clone(),
                });
            gen_template(std::slice::from_ref(node), key)
        }
        Node::Fragment(children, None) => gen_nodes(children),
        Node::Fragment(children, Some(key)) => {
            let inner = gen_nodes(children);
            quote!(::leafy::Html::__keyed(::leafy::__key(#key), #inner))
        }
        Node::Text(s) => quote!(::leafy::Html::Text(::leafy::AttrValue::Static(#s))),
        Node::Block(tokens) => {
            let e = block_expr(tokens.clone());
            quote_spanned!(tokens.span()=> {
                #[allow(unused_imports)]
                use ::leafy::{__SpecAny as _, __SpecStr as _};
                match ::leafy::__W(#e) { __w => (&__w).__kind().__to_html(__w) }
            })
        }
        Node::Iter(tokens) => {
            let e = block_expr(tokens.clone());
            quote!(<::leafy::Html as ::std::iter::FromIterator<_>>::from_iter(#e))
        }
        Node::Component(c) => gen_component(c),
        Node::If(i) => gen_if(i),
        Node::For(f) => {
            let For { pat, expr, body } = f;
            let body = gen_nodes(body);
            quote!(<::leafy::Html as ::std::iter::FromIterator<_>>::from_iter(
                ::std::iter::IntoIterator::into_iter(#expr).map(|#pat| #body)
            ))
        }
    }
}

fn gen_if(i: &If) -> TokenStream {
    let cond = &i.cond;
    let then = gen_nodes(&i.then);
    let els = match &i.els {
        Some(e) => gen_node(e),
        None => quote!(<::leafy::Html as ::std::default::Default>::default()),
    };
    quote!(if #cond { #then } else { #els })
}

/// A component's children: as in yew, a list with an item per child (and
/// per item of a `for`), unless there is only one.
fn gen_children(nodes: &[Node]) -> TokenStream {
    if nodes.len() <= 1 {
        return gen_nodes(nodes);
    }
    let items = nodes.iter().map(|n| {
        let html = gen_node(n);
        if matches!(n, Node::For(_) | Node::Iter(_)) {
            quote!(::leafy::__children_extend(&mut __c, #html);)
        } else {
            quote!(::leafy::__children_push(&mut __c, #html);)
        }
    });
    quote!({
        let mut __c = ::std::vec::Vec::new();
        #(#items)*
        ::leafy::Html::__from_vec(__c)
    })
}

fn prop_ident(ident: &Ident) -> Ident {
    let s = ident.to_string();
    let s = s.trim_start_matches("r#");
    if syn::parse_str::<Ident>(s).is_err() {
        Ident::new_raw(s, ident.span())
    } else {
        Ident::new(s, ident.span())
    }
}

fn gen_component(c: &Component) -> TokenStream {
    let path = &c.path;
    let key = match &c.key {
        Some(k) => quote!(::std::option::Option::Some(::leafy::__key(#k))),
        None => quote!(::std::option::Option::None),
    };
    let val = |v: &AttrVal| match v {
        AttrVal::Lit(l) => l.to_token_stream(),
        AttrVal::Expr(e) => e.clone(),
    };
    let children = c.children.as_ref().map(|ch| gen_children(ch));
    let props = if let Some(base) = &c.base {
        let sets = c.props.iter().map(|(name, v)| {
            let name = prop_ident(name);
            let v = val(v);
            quote!(__p.#name = ::leafy::IntoPropValue::into_prop_value(#v);)
        });
        let children = children.map(|ch| quote!(__p.children = #ch;));
        quote!({
            #[allow(unused_mut)]
            let mut __p = #base;
            #(#sets)*
            #children
            __p
        })
    } else {
        let sets = c.props.iter().map(|(name, v)| {
            let name = prop_ident(name);
            let v = val(v);
            quote_spanned!(name.span()=> .#name(#v))
        });
        let children = children.map(|ch| quote!(.children(#ch)));
        return quote!(::leafy::__comp_b::<#path, _>(#key, <<#path as ::leafy::BaseComponent>::Properties as ::leafy::Properties>::builder()
            #(#sets)* #children));
    };
    quote!(::leafy::__comp::<#path>(#key, #props))
}

// ---------------------------------------------------------------- templates

/// A node of a template being built.
enum T {
    El(TEl),
    Text(String),
    /// A hole whose value is an `Html` expression (as a `Dyn`).
    Hole(TokenStream),
    Comment,
}

struct TEl {
    tag: String,
    statics: Vec<(String, Option<String>)>,
    /// (op, name, `Dyn` expression)
    holes: Vec<(char, String, TokenStream)>,
    children: Vec<T>,
    /// Its content is text the HTML parser doesn't unescape (`<style>`,
    /// `<script>`, ...), so it's written as is.
    raw: bool,
}

const BOOLEAN: &[&str] = &[
    "allowfullscreen",
    "async",
    "autofocus",
    "autoplay",
    "checked",
    "controls",
    "default",
    "defer",
    "disabled",
    "formnovalidate",
    "hidden",
    "inert",
    "ismap",
    "itemscope",
    "loop",
    "multiple",
    "muted",
    "nomodule",
    "novalidate",
    "open",
    "playsinline",
    "readonly",
    "required",
    "reversed",
    "selected",
    "truespeed",
    "webkitdirectory",
];

fn is_known_event(name: &str) -> bool {
    event_type(name) != "Event"
        || matches!(
            name,
            "abort"
                | "beforeinput"
                | "cancel"
                | "canplay"
                | "change"
                | "close"
                | "copy"
                | "cut"
                | "ended"
                | "error"
                | "invalid"
                | "load"
                | "paste"
                | "pause"
                | "play"
                | "playing"
                | "reset"
                | "resize"
                | "scroll"
                | "scrollend"
                | "select"
                | "toggle"
        )
}

thread_local! {
    /// Warnings found while generating.
    static WARNINGS: std::cell::RefCell<Vec<TokenStream>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Emits a warning (as the use of a deprecated item: there is no stable
/// way for a procedural macro to warn).
fn warn(span: Span, msg: &str) {
    WARNINGS.with_borrow_mut(|w| {
        w.push(quote_spanned!(span=> {
            #[deprecated(note = #msg)]
            #[allow(non_upper_case_globals)]
            const html_warning: () = ();
            let () = html_warning;
        }))
    });
}

/// The expression for the root nodes of `html!` (or, if `nested`, of
/// `html_nested!`, which makes a lone component a `VChild`).
pub fn gen_root(nodes: &[Node], nested: bool) -> TokenStream {
    WARNINGS.take();
    let out = match nodes {
        [Node::Component(c)] if nested => {
            let path = &c.path;
            let html = gen_component(c);
            quote!(::leafy::VChild::<#path>::__from_html(#html))
        }
        nodes => gen_nodes(nodes),
    };
    let warnings = WARNINGS.take();
    if warnings.is_empty() {
        out
    } else {
        quote!({ #(#warnings)* #out })
    }
}

fn event_type(name: &str) -> &'static str {
    match name {
        "auxclick" | "click" | "contextmenu" | "dblclick" | "mousedown" | "mouseenter"
        | "mouseleave" | "mousemove" | "mouseout" | "mouseover" | "mouseup" => "MouseEvent",
        "drag" | "dragend" | "dragenter" | "dragexit" | "dragleave" | "dragover" | "dragstart"
        | "drop" => "DragEvent",
        "blur" | "focus" | "focusin" | "focusout" => "FocusEvent",
        "keydown" | "keypress" | "keyup" => "KeyboardEvent",
        "loadstart" | "progress" | "loadend" => "ProgressEvent",
        "wheel" => "WheelEvent",
        "input" => "InputEvent",
        "submit" => "SubmitEvent",
        "animationcancel" | "animationend" | "animationiteration" | "animationstart" => {
            "AnimationEvent"
        }
        "gotpointercapture" | "lostpointercapture" | "pointercancel" | "pointerdown"
        | "pointerenter" | "pointerleave" | "pointermove" | "pointerout" | "pointerover"
        | "pointerup" => "PointerEvent",
        "touchcancel" | "touchend" | "touchmove" | "touchstart" => "TouchEvent",
        "transitioncancel" | "transitionend" | "transitionrun" | "transitionstart" => {
            "TransitionEvent"
        }
        _ => "Event",
    }
}

const P_CLOSERS: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "center",
    "details",
    "dialog",
    "dir",
    "div",
    "dl",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hgroup",
    "hr",
    "listing",
    "main",
    "menu",
    "nav",
    "ol",
    "p",
    "pre",
    "search",
    "section",
    "summary",
    "table",
    "ul",
    "xmp",
];

const TABLE_PARTS: &[&str] = &[
    "tr", "td", "th", "tbody", "thead", "tfoot", "caption", "colgroup", "col",
];
const TABLE_CONTAINERS: &[&str] = &["table", "tbody", "thead", "tfoot", "tr", "colgroup"];
const HEADINGS: &[&str] = &["h1", "h2", "h3", "h4", "h5", "h6"];
/// Elements whose content (in HTML) is text only.
const RAW_TEXT: &[&str] = &[
    "style", "script", "textarea", "title", "xmp", "iframe", "noembed", "noframes",
];
/// Of those, the ones whose text isn't unescaped.
const RAW_UNESCAPED: &[&str] = &["style", "script", "xmp", "iframe", "noembed", "noframes"];

/// HTML's "special" elements (which stop the parser's search for an open
/// `<li>`, `<dd>` or `<dt>` to close).
const SPECIAL: &[&str] = &[
    "address",
    "applet",
    "area",
    "article",
    "aside",
    "base",
    "basefont",
    "bgsound",
    "blockquote",
    "body",
    "br",
    "button",
    "caption",
    "center",
    "col",
    "colgroup",
    "dd",
    "details",
    "dir",
    "div",
    "dl",
    "dt",
    "embed",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "frame",
    "frameset",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "header",
    "hgroup",
    "hr",
    "html",
    "iframe",
    "img",
    "input",
    "keygen",
    "li",
    "link",
    "listing",
    "main",
    "marquee",
    "menu",
    "meta",
    "nav",
    "noembed",
    "noframes",
    "noscript",
    "object",
    "ol",
    "p",
    "param",
    "plaintext",
    "pre",
    "script",
    "search",
    "section",
    "select",
    "source",
    "style",
    "summary",
    "table",
    "tbody",
    "td",
    "template",
    "textarea",
    "tfoot",
    "th",
    "thead",
    "title",
    "tr",
    "track",
    "ul",
    "wbr",
    "xmp",
];

/// Whether the HTML parser would not put `child` where it is (inside
/// `ancestors`, innermost last), so it must be built separately.
fn needs_split(ancestors: &[&str], child: &str) -> bool {
    let Some(&parent) = ancestors.last() else {
        return false;
    };
    if P_CLOSERS.contains(&child) || ["li", "dd", "dt", "plaintext"].contains(&child) {
        for a in ancestors.iter().rev() {
            if *a == "p" {
                return true;
            }
            if ["button", "table", "td", "th", "caption", "template"].contains(a) {
                break;
            }
        }
    }
    let proper_parent: &[&str] = match child {
        "tr" => &["tbody", "thead", "tfoot"],
        "td" | "th" => &["tr"],
        "tbody" | "thead" | "tfoot" | "caption" | "colgroup" => &["table"],
        "col" => &["colgroup"],
        _ => &[],
    };
    if TABLE_PARTS.contains(&child) && !proper_parent.contains(&parent) {
        return true;
    }
    if TABLE_CONTAINERS.contains(&parent) && !TABLE_PARTS.contains(&child) {
        return true;
    }
    if ["a", "form", "button"].contains(&child) && ancestors.contains(&child) {
        return true;
    }
    if HEADINGS.contains(&child) && HEADINGS.contains(&parent) {
        return true;
    }
    if parent == "select" && !["option", "optgroup", "hr"].contains(&child) {
        return true;
    }
    if child == "option" && parent == child {
        return true;
    }
    // An `<li>` (`<dd>`/`<dt>`) closes an open one, found through any
    // non-special elements, `<address>`, `<div>` and `<p>`.
    let closes: &[&str] = match child {
        "li" => &["li"],
        "dd" | "dt" => &["dd", "dt"],
        _ => &[],
    };
    if !closes.is_empty() {
        for a in ancestors.iter().rev() {
            if closes.contains(a) {
                return true;
            }
            if SPECIAL.contains(a) && !["address", "div", "p"].contains(a) {
                break;
            }
        }
    }
    false
}

/// The namespace elements of a template are parsed in.
#[derive(Clone, Copy, PartialEq)]
enum Ns {
    Html,
    Svg,
    Math,
}

thread_local! {
    /// The namespace of the template being built (and so the context of
    /// content built separately within it).
    static NS: std::cell::Cell<Ns> = const { std::cell::Cell::new(Ns::Html) };
}

/// SVG elements that aren't also HTML elements (`a`, `script`, `style` and
/// `title` are both).
const SVG_ONLY: &[&str] = &[
    "animate",
    "animateMotion",
    "animateTransform",
    "circle",
    "clipPath",
    "defs",
    "desc",
    "ellipse",
    "feBlend",
    "feColorMatrix",
    "feComponentTransfer",
    "feComposite",
    "feConvolveMatrix",
    "feDiffuseLighting",
    "feDisplacementMap",
    "feDistantLight",
    "feDropShadow",
    "feFlood",
    "feFuncA",
    "feFuncB",
    "feFuncG",
    "feFuncR",
    "feGaussianBlur",
    "feImage",
    "feMerge",
    "feMergeNode",
    "feMorphology",
    "feOffset",
    "fePointLight",
    "feSpecularLighting",
    "feSpotLight",
    "feTile",
    "feTurbulence",
    "filter",
    "foreignObject",
    "g",
    "image",
    "line",
    "linearGradient",
    "marker",
    "mask",
    "metadata",
    "mpath",
    "path",
    "pattern",
    "polygon",
    "polyline",
    "radialGradient",
    "rect",
    "set",
    "stop",
    "switch",
    "symbol",
    "text",
    "textPath",
    "tspan",
    "use",
    "view",
];

const MATH_ONLY: &[&str] = &[
    "annotation",
    "annotation-xml",
    "maction",
    "menclose",
    "merror",
    "mfenced",
    "mfrac",
    "mi",
    "mmultiscripts",
    "mn",
    "mo",
    "mover",
    "mpadded",
    "mphantom",
    "mprescripts",
    "mroot",
    "mrow",
    "ms",
    "mspace",
    "msqrt",
    "mstyle",
    "msub",
    "msubsup",
    "msup",
    "mtable",
    "mtd",
    "mtext",
    "mtr",
    "munder",
    "munderover",
    "semantics",
];

/// The namespace of content inside `tags` (innermost last), if they decide
/// it.
fn ns_of_tags(tags: &[String]) -> Option<Ns> {
    for t in tags.iter().rev() {
        match t.as_str() {
            "foreignObject" => return Some(Ns::Html),
            "svg" => return Some(Ns::Svg),
            "math" => return Some(Ns::Math),
            _ => {}
        }
    }
    None
}

/// Generates `f` with `ns` as the context namespace.
fn with_ns<R>(ns: Ns, f: impl FnOnce() -> R) -> R {
    let prev = NS.replace(ns);
    let r = f();
    NS.set(prev);
    r
}

/// Content built separately, inside `tags` of the current template.
fn gen_nested(node: &Node, tags: &[String]) -> TokenStream {
    let ns = ns_of_tags(tags).unwrap_or(NS.get());
    with_ns(ns, || gen_node(node))
}

fn child_dyn(html: TokenStream) -> TokenStream {
    quote!(::leafy::__child_html(#html))
}

fn lower(node: &Node, ancestors: &mut Vec<&'static str>, out: &mut Vec<T>, tags: &mut Vec<String>) {
    match node {
        Node::Element(e) => {
            let parent_tags: Vec<&str> = tags.iter().map(String::as_str).collect();
            let ns = ns_of_tags(tags).unwrap_or(NS.get());
            if ns == Ns::Html && needs_split(&parent_tags, &e.tag) {
                warn(
                    e.span,
                    &format!(
                        "an HTML parser wouldn't put `<{}>` here, in `<{}>` (it would move it, so a server-rendered page would differ), so it's built as a template of its own",
                        e.tag,
                        parent_tags.last().unwrap_or(&""),
                    ),
                );
                out.push(T::Hole(child_dyn(gen_nested(node, tags))));
                return;
            }
            out.push(T::El(lower_element(e, ancestors, tags)));
        }
        Node::Fragment(children, None) => {
            for c in children {
                lower(c, ancestors, out, tags);
            }
        }
        Node::Text(s) => {
            let in_table = tags
                .last()
                .is_some_and(|t| TABLE_CONTAINERS.contains(&t.as_str()));
            // (Only HTML's whitespace stays in a table.)
            let blank = s
                .chars()
                .all(|c| matches!(c, ' ' | '\t' | '\n' | '\x0c' | '\r'));
            if in_table && !blank {
                warn(
                    Span::call_site(),
                    &format!(
                        "an HTML parser wouldn't put text directly in `<{}>` (it would move it, so a server-rendered page would differ), so it's built as a template of its own",
                        tags.last().map_or("", String::as_str),
                    ),
                );
                out.push(T::Hole(child_dyn(gen_nested(node, tags))));
            } else {
                out.push(T::Text(s.clone()));
            }
        }
        Node::Block(tokens) => {
            let e = block_expr(tokens.clone());
            out.push(T::Hole(quote_spanned!(tokens.span()=> {
                #[allow(unused_imports)]
                use ::leafy::{__SpecAny as _, __SpecStr as _};
                match ::leafy::__W(#e) { __w => (&__w).__kind().__push(__w) }
            })))
        }
        _ => out.push(T::Hole(child_dyn(gen_nested(node, tags)))),
    }
}

fn lower_element(e: &Element, ancestors: &mut Vec<&'static str>, tags: &mut Vec<String>) -> TEl {
    let tag = e.tag.clone();
    let mut statics = Vec::new();
    let mut holes = Vec::new();
    let is_input = tag == "input";
    let has_value_prop = is_input || tag == "textarea";
    for (i, a) in e.attrs.iter().enumerate() {
        let name = a.name.as_str();
        let span = a.span;
        // (yew allows several listeners for an event.)
        if !name.starts_with("on") && e.attrs[..i].iter().any(|b| b.name == a.name) {
            warn(span, &format!("`{name}` is given more than once"));
        }
        let expr = match &a.value {
            AttrVal::Lit(l) => l.to_token_stream(),
            AttrVal::Expr(e) => e.clone(),
        };
        if name == "key" {
            continue;
        }
        if name == "ref" {
            holes.push((
                'r',
                String::new(),
                quote_spanned!(span=> ::leafy::__ref(#expr)),
            ));
            continue;
        }
        if let Some(event) = name.strip_prefix("on") {
            let lower = event.to_ascii_lowercase();
            // Event names are case sensitive (as in yew).
            if lower != event && is_known_event(&lower) {
                warn(
                    span,
                    &format!(
                        "`{name}` listens for an event named `{event}`; did you mean `on{lower}`?"
                    ),
                );
            }
            let ty = Ident::new(event_type(event), span);
            let op = if matches!(event, "touchstart" | "touchmove" | "scroll") {
                'E'
            } else {
                'e'
            };
            holes.push((
                op,
                event.to_string(),
                quote_spanned!(span=> ::leafy::__on::<::leafy::web_sys::#ty, _>(#expr)),
            ));
            continue;
        }
        // Boolean properties, which (unlike their attributes) reflect the
        // element's current state.
        let bool_prop = match name {
            "checked" => is_input,
            "selected" => tag == "option",
            "muted" => tag == "video" || tag == "audio",
            _ => false,
        };
        // (A literal `selected` or `muted` sets the default, as an attribute.)
        if bool_prop && (name == "checked" || !matches!(a.value, AttrVal::Lit(_))) {
            holes.push((
                'p',
                name.to_string(),
                quote_spanned!(span=> ::leafy::__bool(#expr)),
            ));
            continue;
        }
        if name == "value" && has_value_prop {
            holes.push((
                'p',
                name.to_string(),
                quote_spanned!(span=> ::leafy::__attr(#expr)),
            ));
            continue;
        }
        let is_bool = BOOLEAN.contains(&name);
        match &a.value {
            AttrVal::Lit(Lit::Bool(b)) if is_bool => {
                if b.value {
                    statics.push((name.to_string(), None));
                }
            }
            AttrVal::Lit(l) if !is_bool => {
                if let Some(text) = lit_text(l) {
                    statics.push((name.to_string(), Some(text)));
                }
            }
            _ => {
                let f = if is_bool {
                    quote_spanned!(span=> ::leafy::__bool(#expr))
                } else if name == "class" {
                    quote_spanned!(span=> ::leafy::__class(#expr))
                } else {
                    quote_spanned!(span=> ::leafy::__attr(#expr))
                };
                holes.push(('a', name.to_string(), f));
            }
        }
    }
    // Evaluate in yew's order: value and checked, ref, class, other
    // attributes, listeners.
    holes.sort_by_key(|(op, name, _)| match op {
        'p' => 0,
        'r' => 1,
        'a' if name == "class" => 2,
        'a' => 3,
        _ => 4,
    });
    let html = ns_of_tags(tags).unwrap_or(NS.get()) == Ns::Html;
    let text_only = html && RAW_TEXT.contains(&tag.as_str());
    let mut children = Vec::new();
    if !VOID.contains(&tag.as_str()) {
        tags.push(tag.clone());
        let dynamic = e.children.iter().any(|c| !matches!(c, Node::Text(_)));
        if text_only && e.children.iter().any(|c| matches!(c, Node::Element(_))) {
            let msg = format!("the content of <{tag}> is text: it can't contain elements");
            children.push(T::Hole(
                quote_spanned!(e.span=> ::core::compile_error!(#msg)),
            ));
        } else if text_only && dynamic && e.children.len() > 1 {
            // Dynamic text mixed with more: built as one hole (its own
            // template of text and anchors).
            let ns = ns_of_tags(tags).unwrap_or(NS.get());
            children.push(T::Hole(child_dyn(with_ns(ns, || gen_nodes(&e.children)))));
        } else if tag == "template" && !e.children.is_empty() && dynamic {
            // (Its children would be in its `content`, out of the walk.)
            children.push(T::Hole(quote_spanned!(e.span=> ::core::compile_error!("dynamic content in a <template> isn't supported"))));
        } else {
            for c in &e.children {
                lower(c, ancestors, &mut children, tags);
            }
        }
        tags.pop();
    }
    TEl {
        raw: text_only && RAW_UNESCAPED.contains(&tag.as_str()),
        tag,
        statics,
        holes,
        children,
    }
}

/// Merges adjacent text and adds comment anchors where a hole has no
/// element after it (except at the end of an element, where it appends).
fn finalize(children: Vec<T>, in_element: bool) -> Vec<T> {
    let mut merged: Vec<T> = Vec::new();
    for c in children {
        match (merged.last_mut(), c) {
            (_, T::Text(s)) if s.is_empty() => {}
            (Some(T::Text(a)), T::Text(b)) => a.push_str(&b),
            (_, T::El(mut e)) => {
                let ch = std::mem::take(&mut e.children);
                e.children = finalize(ch, true);
                merged.push(T::El(e));
            }
            (_, c) => merged.push(c),
        }
    }
    let mut out = Vec::new();
    let n = merged.len();
    let mut iter = merged.into_iter().peekable();
    let mut i = 0;
    while let Some(c) = iter.next() {
        let is_hole = matches!(c, T::Hole(_));
        out.push(c);
        if is_hole {
            let next_is_el = matches!(iter.peek(), Some(T::El(_)));
            let last = i + 1 == n;
            if !next_is_el && !(last && in_element) {
                out.push(T::Comment);
            }
        }
        i += 1;
    }
    out
}

fn escape_text(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
}

fn escape_attr(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
}

struct Emitter {
    html: String,
    walk: String,
    dyns: Vec<TokenStream>,
    /// For server rendering: where in `html` each hole's output goes.
    offsets: Vec<usize>,
    /// Offset of the start of each node (by address).
    node_start: std::collections::HashMap<usize, usize>,
    /// Offset of the `>` ending each element's start tag (by address).
    tag_end: std::collections::HashMap<usize, usize>,
    /// Offset of each element's end tag (by address).
    close_start: std::collections::HashMap<usize, usize>,
    /// Where the comment anchors are (which server rendering leaves out).
    comments: Vec<usize>,
    /// For server rendering, each hole's kind (see the runtime's `ssr.rs`)
    /// and attribute name.
    kinds: Vec<(char, String)>,
}

impl Emitter {
    fn emit_html(&mut self, children: &[T], raw: bool) {
        for c in children {
            self.node_start
                .insert(c as *const T as usize, self.html.len());
            match c {
                T::El(e) => {
                    self.html.push('<');
                    self.html.push_str(&e.tag);
                    for (name, value) in &e.statics {
                        self.html.push(' ');
                        self.html.push_str(name);
                        if let Some(v) = value {
                            // Always quoted: the browser doesn't need it, but
                            // other parsers of server-rendered pages may (and
                            // it costs nothing after compression).
                            self.html.push_str("=\"");
                            escape_attr(v, &mut self.html);
                            self.html.push('"');
                        }
                    }
                    self.tag_end
                        .insert(e as *const TEl as usize, self.html.len());
                    self.html.push('>');
                    if !VOID.contains(&e.tag.as_str()) {
                        // The parser drops a newline just after these start
                        // tags: so that it drops this one instead.
                        if ["pre", "listing", "textarea"].contains(&e.tag.as_str())
                            && matches!(e.children.first(), Some(T::Text(t)) if t.starts_with('\n'))
                        {
                            self.html.push('\n');
                        }
                        self.emit_html(&e.children, e.raw);
                        self.close_start
                            .insert(e as *const TEl as usize, self.html.len());
                        self.html.push_str("</");
                        self.html.push_str(&e.tag);
                        self.html.push('>');
                    }
                }
                T::Text(s) => {
                    if raw {
                        self.html.push_str(s);
                    } else {
                        escape_text(s, &mut self.html);
                    }
                }
                T::Comment => {
                    self.comments.push(self.html.len());
                    self.html.push_str("<!>");
                }
                T::Hole(_) => {}
            }
        }
    }

    /// Emits the walk for `children` of the node the cursor is on. Returns
    /// whether anything was emitted (in which case the cursor ends on the
    /// parent again).
    fn emit_walk(
        &mut self,
        children: &[T],
        top: bool,
        parent_el: Option<usize>,
        // Whether the parent's content is text written as is.
        raw_parent: bool,
    ) -> syn::Result<()> {
        // Index (among DOM nodes) of each node, and the hole before it.
        let mut nodes: Vec<(&T, Option<&TokenStream>)> = Vec::new();
        let mut pending: Option<&TokenStream> = None;
        let mut append: Option<&TokenStream> = None;
        for c in children {
            match c {
                T::Hole(h) => {
                    if pending.is_some() {
                        unreachable!("consecutive holes");
                    }
                    pending = Some(h);
                }
                c => nodes.push((c, pending.take())),
            }
        }
        if let Some(h) = pending {
            if top {
                unreachable!("top-level hole without anchor");
            }
            append = Some(h);
        }
        let needs = |(n, h): &(&T, Option<&TokenStream>)| -> bool {
            top || h.is_some() || matches!(n, T::El(e) if el_needs_walk(e))
        };
        let mut cur: Option<usize> = None;
        let mut descended = false;
        for (i, item) in nodes.iter().enumerate() {
            if !needs(item) {
                continue;
            }
            match cur {
                None => {
                    self.walk.push('f');
                    descended = true;
                    for _ in 0..i {
                        self.walk.push('n');
                    }
                }
                Some(c) => {
                    for _ in c..i {
                        self.walk.push('n');
                    }
                }
            }
            cur = Some(i);
            let (node, hole) = item;
            if let Some(h) = hole {
                self.walk.push(if top { 'C' } else { 'c' });
                self.dyns.push((*h).clone());
                self.offsets
                    .push(self.node_start[&(*node as *const T as usize)]);
                self.kinds.push(('c', String::new()));
            }
            if top {
                self.walk.push('R');
            }
            if let T::El(e) = node {
                self.emit_el_walk(e)?;
            }
        }
        if descended {
            self.walk.push('u');
        }
        if let Some(h) = append {
            self.walk.push('l');
            self.dyns.push(h.clone());
            self.offsets.push(self.close_start[&parent_el.unwrap()]);
            self.kinds
                .push((if raw_parent { 's' } else { 'c' }, String::new()));
        }
        Ok(())
    }

    fn emit_el_walk(&mut self, e: &TEl) -> syn::Result<()> {
        for (op, name, expr) in &e.holes {
            self.offsets.push(self.tag_end[&(e as *const TEl as usize)]);
            self.kinds.push(match op {
                'p' if name == "value" && e.tag == "textarea" => ('v', String::new()),
                'a' | 'p' => ('a', name.clone()),
                _ => ('-', String::new()),
            });
            self.walk.push(*op);
            if *op != 'r' {
                self.walk.push_str(name);
                self.walk.push(';');
            }
            self.dyns.push(expr.clone());
        }
        self.emit_walk(&e.children, false, Some(e as *const TEl as usize), e.raw)
    }
}

fn el_needs_walk(e: &TEl) -> bool {
    !e.holes.is_empty()
        || e.children
            .iter()
            .any(|c| matches!(c, T::Hole(_)) || matches!(c, T::El(e) if el_needs_walk(e)))
}

/// A template for `nodes` (at the top level of a template).
fn gen_template(nodes: &[Node], key: Option<TokenStream>) -> TokenStream {
    // Elements are parsed as HTML unless in SVG or MathML (where, as the
    // template has no `<svg>`/`<math>` of its own, it's parsed inside one).
    let first_tag = nodes.iter().find_map(|n| match n {
        Node::Element(e) => Some(e.tag.as_str()),
        _ => None,
    });
    let ns = match NS.get() {
        Ns::Html => match first_tag {
            Some(t) if SVG_ONLY.contains(&t) => Ns::Svg,
            Some(t) if MATH_ONLY.contains(&t) => Ns::Math,
            _ => Ns::Html,
        },
        ns => ns,
    };
    let wrapper = match (ns, first_tag) {
        (_, None) | (Ns::Html, _) | (Ns::Svg, Some("svg")) | (Ns::Math, Some("math")) => None,
        (Ns::Svg, _) => Some("svg"),
        (Ns::Math, _) => Some("math"),
    };
    let mut lowered = Vec::new();
    let mut tags = Vec::new();
    with_ns(ns, || {
        for n in nodes {
            match n {
                // At the top level, an element is never split.
                Node::Element(e) => {
                    lowered.push(T::El(lower_element(e, &mut Vec::new(), &mut tags)))
                }
                n => lower(n, &mut Vec::new(), &mut lowered, &mut tags),
            }
        }
    });
    let children = finalize(lowered, false);
    let mut em = Emitter {
        html: String::new(),
        walk: String::new(),
        dyns: Vec::new(),
        offsets: Vec::new(),
        node_start: Default::default(),
        tag_end: Default::default(),
        close_start: Default::default(),
        comments: Vec::new(),
        kinds: Vec::new(),
    };
    em.emit_html(&children, false);
    if let Err(e) = em.emit_walk(&children, true, None, false) {
        return e.to_compile_error();
    }
    while em.walk.ends_with('u') {
        em.walk.pop();
    }
    let Emitter {
        html,
        walk,
        dyns,
        offsets,
        comments,
        kinds,
        ..
    } = em;
    // For server rendering, without the comments.
    let mut ssr_html = String::with_capacity(html.len());
    let mut last = 0;
    for &c in &comments {
        ssr_html.push_str(&html[last..c]);
        last = c + "<!>".len();
    }
    ssr_html.push_str(&html[last..]);
    let ssr_holes = offsets.iter().zip(&kinds).map(|(&o, (kind, name))| {
        let at = (o - 3 * comments.iter().filter(|&&c| c < o).count()) as u32;
        let kind = *kind as u8;
        quote!(::leafy::__SsrHole(#at, #kind, #name))
    });
    // In the browser, parse inside a wrapper element (which the walk's
    // first op removes).
    let (wasm_html, wasm_walk) = match wrapper {
        Some(w) => (format!("<{w}>{html}"), format!("W{walk}")),
        None => (html.clone(), walk.clone()),
    };
    let tpl = quote!(
        #[cfg(target_arch = "wasm32")]
        static __T: ::leafy::__Tpl = ::leafy::__tpl_new(#wasm_html, #wasm_walk);
        #[cfg(not(target_arch = "wasm32"))]
        static __T: ::leafy::__Tpl = ::leafy::__tpl_new_ssr(#ssr_html, #walk, &[#(#ssr_holes),*]);
    );
    let n = dyns.len();
    if n == 0 {
        let make = match &key {
            Some(k) => {
                quote!(::leafy::Html::__tpl(&__T, ::std::option::Option::Some(::leafy::__key(#k)), ::std::vec::Vec::new()))
            }
            None => quote!(::leafy::Html::__tpl0(&__T, ::std::vec::Vec::new())),
        };
        return quote!({
            #tpl
            #make
        });
    }
    let make = match &key {
        Some(k) => quote!(::leafy::__end_keyed(&__T, ::leafy::__key(#k), __f)),
        None => quote!(::leafy::__end(&__T, __f)),
    };
    quote!({
        #tpl
        let __f = ::leafy::__begin();
        #(#dyns;)*
        #make
    })
}
