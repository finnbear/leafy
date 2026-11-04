//! Server-side rendering: [`Html`] to an HTML string, without a DOM.

use crate::component::{__comp, BaseComponent, CompState, with_ssr};
use crate::css::{self, RenderedCss};
use crate::vdom::{Dyn, Html, SsrHole};
use std::rc::Rc;

/// Renders a component to an HTML string, on the current thread.
///
/// Effects don't run, and the output isn't hydratable. (Unlike yew's, this
/// isn't async: without suspense, rendering finishes at once.)
pub struct ServerRenderer<C: BaseComponent> {
    props: C::Properties,
}

impl<C: BaseComponent> ServerRenderer<C>
where
    C::Properties: Default,
{
    /// With default properties.
    pub fn new() -> Self {
        Self::with_props(Default::default())
    }
}

impl<C: BaseComponent> Default for ServerRenderer<C>
where
    C::Properties: Default,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<C: BaseComponent> ServerRenderer<C> {
    /// With `props`.
    pub fn with_props(props: C::Properties) -> Self {
        Self { props }
    }

    /// Renders the component to an HTML string.
    pub fn render(self) -> String {
        let mut s = String::new();
        self.render_to(&mut s);
        s
    }

    /// Renders the component, appending to `out` (e.g. a buffer reused
    /// between pages).
    pub fn render_to(self, out: &mut String) {
        self.run(out, Head::Off);
    }

    /// Renders the component to an HTML string, with the stylesheets it
    /// uses (of [`css!`](crate::css) and [`global_css!`](crate::global_css)),
    /// for the page's `<head>`.
    pub fn render_with_css(self) -> (String, RenderedCss) {
        css::collect(|| self.render())
    }

    /// Renders the component as a whole page: `<!DOCTYPE html>`, then the
    /// component (an `<html>`, with a `<head>` and a `<body>`), with the
    /// stylesheets it uses (as [`render_with_css`](Self::render_with_css))
    /// as `<style>`s at the end of its `<head>`.
    ///
    /// The end of the `<head>` is the first `</head>` of an `html!`, as it
    /// renders (never one in a value, or raw HTML). Without one, the
    /// stylesheets go just after `<!DOCTYPE html>` (where a browser puts
    /// them in the `<head>`).
    pub fn render_page(self) -> String {
        const DOCTYPE: &str = "<!DOCTYPE html>";
        let mut out = String::from(DOCTYPE);
        let (head, css) = css::collect(|| self.run(&mut out, Head::Searching));
        let at = match head {
            Head::Found(at) => at,
            _ => DOCTYPE.len(),
        };
        out.insert_str(at, &css.styles());
        out
    }

    /// Renders, appending to `out`, finding the end of the `<head>` if
    /// `head` is `Searching`.
    fn run(self, out: &mut String, mut head: Head) -> Head {
        let html = __comp::<C>(None, self.props);
        with_ssr(|| render(&html, None, out, &mut head));
        head
    }
}

/// Where the end of the `<head>` is, in a render looking for it.
#[derive(Clone, Copy)]
enum Head {
    /// Not looking for it.
    Off,
    Searching,
    /// The offset of its `</head>` in the output.
    Found(usize),
}

/// If `s`, to go at `at` in the output, has the end of the `<head>`, notes
/// where. (Out of line: only a whole page's render looks.)
#[cold]
#[inline(never)]
fn search_head(s: &str, at: usize, head: &mut Head) {
    if let Some(end) = find_head_end(s) {
        *head = Head::Found(at + end);
    }
}

/// The offset of the first `</head>` (in any case, and maybe with space
/// before the `>`) in `s`.
fn find_head_end(s: &str) -> Option<usize> {
    s.match_indices("</").map(|(i, _)| i).find(|&i| {
        s.as_bytes().get(i + 2..i + 7).is_some_and(|tag| {
            tag[..4].eq_ignore_ascii_case(b"head")
                && matches!(tag[4], b'>' | b' ' | b'\t' | b'\n' | b'\r' | b'\x0c')
        })
    })
}

fn escape(s: &str, attr: bool, out: &mut String) {
    // Runs of text needing no escape are copied at once.
    let mut start = 0;
    for (i, b) in s.bytes().enumerate() {
        let entity = match b {
            b'&' => "&amp;",
            b'<' if !attr => "&lt;",
            b'>' if !attr => "&gt;",
            b'"' if attr => "&quot;",
            _ => continue,
        };
        out.push_str(&s[start..i]);
        out.push_str(entity);
        start = i + 1;
    }
    out.push_str(&s[start..]);
}

/// Static template text (with any pending `content` after the start tag it
/// begins with). (Inlined, as it's on the hot path.)
#[inline(always)]
fn push_static(s: &str, out: &mut String, content: &mut Option<crate::AttrValue>, head: &mut Head) {
    let s = match content.take() {
        Some(c) if s.starts_with('>') => {
            out.push('>');
            escape(&c, false, out);
            &s[1..]
        }
        c => {
            *content = c;
            s
        }
    };
    if let Head::Searching = head {
        search_head(s, out.len(), head);
    }
    out.push_str(s);
}

/// How deeply nested (components, lists and templates within templates)
/// content may be: deeper content is left out, rather than overflowing the
/// stack, which would abort the process.
const MAX_DEPTH: u32 = 512;

thread_local! {
    static DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Whether `text` can be written as is as the content of a `<script>` or
/// `<style>`: it can't end the element or start a comment.
fn raw_text_safe(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    !(lower.contains("</script") || lower.contains("</style") || lower.contains("<!--"))
}

/// Renders `html`, appending to `out`, and finding the end of the `<head>`
/// if `head` is `Searching`.
fn render(html: &Html, owner: Option<&Rc<CompState>>, out: &mut String, head: &mut Head) {
    struct Depth;
    impl Drop for Depth {
        fn drop(&mut self) {
            DEPTH.set(DEPTH.get() - 1);
        }
    }
    if DEPTH.get() >= MAX_DEPTH {
        return;
    }
    DEPTH.set(DEPTH.get() + 1);
    let _depth = Depth;
    match html {
        Html::Empty | Html::VRef(_) => {}
        Html::Text(t) => escape(t, false, out),
        Html::Raw(r) => out.push_str(r),
        Html::List(l) => {
            for c in &l.children {
                render(c, owner, out, head);
            }
        }
        Html::Comp(vcomp) => {
            let state = CompState::new_ssr(vcomp, owner);
            if let Some(html) = state.run() {
                render(&html, Some(&state), out, head);
            }
            state.hooks.take();
        }
        Html::Tpl(t) => {
            let tpl = t.tpl;
            let mut pos = 0;
            // A textarea's content, to go after its start tag.
            let mut content = None;
            for (&SsrHole(at, kind, name), d) in tpl.ssr.iter().zip(&t.dyns) {
                let at = at as usize;
                push_static(&tpl.html[pos..at], out, &mut content, head);
                pos = at;
                match (kind, d) {
                    // A textarea's value is its content.
                    (b'v', Dyn::Attr(v)) => content = v.clone(),
                    (b'a', Dyn::Attr(Some(v))) => {
                        // Always `name="value"` (`=""` for a boolean or empty
                        // value): not every parser of pages takes less.
                        out.push(' ');
                        out.push_str(name);
                        out.push_str("=\"");
                        escape(v, true, out);
                        out.push('"');
                    }
                    // The content of `<style>` and `<script>` isn't escaped,
                    // unless it could end the element (then it's escaped, as
                    // yew does, which is safe but breaks it).
                    (b'a', Dyn::Class(classes)) => {
                        css::note(classes.css.as_deref().map_or(&[], |css| css));
                        out.push(' ');
                        out.push_str(name);
                        out.push_str("=\"");
                        escape(classes.as_str(), true, out);
                        out.push('"');
                    }
                    (b's', Dyn::Child(Html::Text(text))) if raw_text_safe(text) => {
                        out.push_str(text)
                    }
                    (b'c' | b's', Dyn::Child(child)) => render(child, owner, out, head),
                    _ => {}
                }
            }
            push_static(&tpl.html[pos..], out, &mut content, head);
        }
    }
}
