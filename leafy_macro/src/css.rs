//! `css!` and `global_css!`: stylesheets written at compile time.

use proc_macro2::TokenStream;
use quote::quote;
use stylist_core::ast::{Sheet, ToStyleStr};
use syn::LitStr;

/// What every `css!` class starts with ("scoped class"), so a server can
/// recognize classes whose stylesheet must be on the page.
const CLASS_PREFIX: &str = "sc-";

/// `css!` (`global` false) or `global_css!`: the literal is parsed as
/// stylist's `css!` parses it (so the same CSS is accepted: `&` for the
/// class, nested blocks, `@media`), and written out as stylist writes it,
/// scoped to a class named for its text unless global, then minified.
pub fn css(input: TokenStream, global: bool) -> TokenStream {
    let literal = match syn::parse2::<LitStr>(input) {
        Ok(literal) => literal,
        Err(e) => return e.to_compile_error(),
    };
    let sheet = match literal.value().parse::<Sheet>() {
        Ok(sheet) => sheet,
        Err(e) => return syn::Error::new(literal.span(), e.to_string()).to_compile_error(),
    };
    let (class, css) = if global {
        (String::new(), minify(&sheet.to_style_str(None)))
    } else {
        // Named for its rules, so a class is the same in every build, and on
        // the server and in the browser.
        let hash = fnv1a(&minify(&sheet.to_style_str(Some(CLASS_PREFIX))));
        let class = format!("{CLASS_PREFIX}{:010x}", hash & 0xff_ffff_ffff);
        let css = minify(&sheet.to_style_str(Some(&class)));
        (class, css)
    };
    // Put on the page as a `<style>`'s text, which it mustn't end.
    if css.to_ascii_lowercase().contains("</style") {
        return syn::Error::new(literal.span(), "a stylesheet can't contain `</style`")
            .to_compile_error();
    }
    let make = if global {
        quote!(&__CSS)
    } else {
        quote!(::leafy::CssClass::__new(&__CSS))
    };
    quote!({
        static __CSS: ::leafy::Css = ::leafy::Css::__new(#class, #css);
        #make
    })
}

/// `css` with each run of whitespace one space, and none where it can't
/// matter: around `{`, `}` and `;`, before `,`, and after `:` and `,`. Not
/// before `:`, where a space is a descendant combinator (`a :hover`). Quoted
/// strings are copied as they are.
fn minify(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut chars = css.chars().peekable();
    let mut pending_space = false;
    while let Some(c) = chars.next() {
        if c.is_whitespace() {
            pending_space = true;
            continue;
        }
        if pending_space {
            let before = out.chars().last();
            let drop = matches!(before, None | Some('{' | '}' | ';' | ':' | ','))
                || matches!(c, '{' | '}' | ';' | ',');
            if !drop {
                out.push(' ');
            }
            pending_space = false;
        }
        if c == '}' && out.ends_with(';') {
            out.pop();
        }
        out.push(c);
        if c == '"' || c == '\'' {
            while let Some(inner) = chars.next() {
                out.push(inner);
                if inner == '\\' {
                    if let Some(escaped) = chars.next() {
                        out.push(escaped);
                    }
                } else if inner == c {
                    break;
                }
            }
        }
    }
    out
}

fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ byte as u64).wrapping_mul(0x0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::minify;

    #[test]
    fn minifies() {
        assert_eq!(
            minify(".a {\n  color: red;\n  margin: 0 1px ;\n}\n"),
            ".a{color:red;margin:0 1px}"
        );
        assert_eq!(
            minify(".a :hover , .b{content: \"a  b\"}"),
            ".a :hover,.b{content:\"a  b\"}"
        );
    }
}
