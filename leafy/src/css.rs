//! Stylesheets written at compile time by [`css!`](crate::css) and
//! [`global_css!`](crate::global_css), put on the page the first time
//! they're needed: nothing is parsed, hashed or looked up at runtime.
//!
//! A stylesheet can't contain `</style`, which would end its `<style>`:
//!
//! ```compile_fail
//! leafy::css!("&::after { content: \"</style>\"; }");
//! ```

use crate::{Classes, IntoPropValue};
#[cfg(not(target_arch = "wasm32"))]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use std::sync::atomic::{AtomicBool, Ordering};

/// A [`css!`](crate::css) class. Its stylesheet goes on the page when it
/// first becomes one of an element's [`Classes`], not where the macro is
/// written, so stylesheets land in the order they're first used.
#[derive(Clone, Copy)]
pub struct CssClass(&'static Css);

impl CssClass {
    #[doc(hidden)]
    pub const fn __new(css: &'static Css) -> Self {
        Self(css)
    }

    /// The stylesheet.
    pub fn css(&self) -> &'static Css {
        self.0
    }
}

impl From<CssClass> for Classes {
    fn from(class: CssClass) -> Self {
        #[allow(unused_mut)]
        let mut classes = Classes::from(class.0.register());
        #[cfg(not(target_arch = "wasm32"))]
        {
            classes.css = Some(Box::new(vec![class.0]));
        }
        classes
    }
}

impl IntoPropValue<Classes> for CssClass {
    fn into_prop_value(self) -> Classes {
        self.into()
    }
}

impl IntoPropValue<Option<Classes>> for CssClass {
    fn into_prop_value(self) -> Option<Classes> {
        Some(self.into())
    }
}

/// A stylesheet from [`css!`](crate::css) or
/// [`global_css!`](crate::global_css): its CSS, and the class its rules are
/// scoped to.
pub struct Css {
    /// Empty for a global stylesheet.
    class: &'static str,
    css: &'static str,
    #[cfg(target_arch = "wasm32")]
    registered: AtomicBool,
}

#[cfg(not(target_arch = "wasm32"))]
thread_local! {
    /// The stylesheets used so far by the server render in progress on this
    /// thread, if it collects them.
    static RENDERING: RefCell<Option<Vec<&'static Css>>> = const { RefCell::new(None) };
}

impl Css {
    #[doc(hidden)]
    pub const fn __new(class: &'static str, css: &'static str) -> Self {
        Self {
            class,
            css,
            #[cfg(target_arch = "wasm32")]
            registered: AtomicBool::new(false),
        }
    }

    /// Puts the stylesheet on the page, the first time, and returns its
    /// class (empty for a global stylesheet).
    ///
    /// In the browser, it goes in a `<style>` of its own at the end of
    /// `<head>`. On the server, called while rendering, it's one of the
    /// render's [`RenderedCss`] (otherwise, it does nothing there).
    pub fn register(&'static self) -> &'static str {
        #[cfg(target_arch = "wasm32")]
        if !self.registered.swap(true, Ordering::Relaxed) {
            crate::dom::add_style(self.css);
        }
        #[cfg(not(target_arch = "wasm32"))]
        note(std::slice::from_ref(&self));
        self.class
    }

    /// The class its rules are scoped to; empty for a global stylesheet.
    pub fn class(&self) -> &'static str {
        self.class
    }

    /// The (minified) CSS.
    pub fn css(&self) -> &'static str {
        self.css
    }
}

/// The stylesheets a server render used (see
/// [`ServerRenderer::render_with_css`](crate::ServerRenderer::render_with_css)),
/// in the order they were first used, each once.
///
/// Those of the [`css!`](crate::css) classes in its output, wherever their
/// [`Html`](crate::Html) was built (in properties, or cached from an earlier
/// render), and any stylesheet registered while rendering (as a
/// [`global_css!`](crate::global_css) is).
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Default)]
pub struct RenderedCss(Vec<&'static Css>);

#[cfg(not(target_arch = "wasm32"))]
impl RenderedCss {
    /// Appends each stylesheet as a `<style>` to `out`, in order (for the
    /// page's `<head>`).
    pub fn write_styles(&self, out: &mut String) {
        for css in &self.0 {
            out.push_str("<style>");
            out.push_str(css.css);
            out.push_str("</style>");
        }
    }

    /// Each stylesheet as a `<style>`, in order (for the page's `<head>`).
    pub fn styles(&self) -> String {
        let mut out = String::new();
        self.write_styles(&mut out);
        out
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl std::ops::Deref for RenderedCss {
    type Target = [&'static Css];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl IntoIterator for RenderedCss {
    type Item = &'static Css;
    type IntoIter = std::vec::IntoIter<&'static Css>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<'a> IntoIterator for &'a RenderedCss {
    type Item = &'a &'static Css;
    type IntoIter = std::slice::Iter<'a, &'static Css>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

/// Adds those of `from` not yet in `into` to it, in order.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn merge(into: &mut Vec<&'static Css>, from: &[&'static Css]) {
    for &css in from {
        // The same stylesheet, or another `css!` of the same rules.
        if !into.iter().any(|other| {
            std::ptr::eq(*other, css) || (other.class == css.class && other.css == css.css)
        }) {
            into.push(css);
        }
    }
}

/// Notes that the render in progress, if it collects stylesheets, used
/// `css`.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn note(css: &[&'static Css]) {
    RENDERING.with_borrow_mut(|rendering| {
        if let Some(rendering) = rendering {
            merge(rendering, css);
        }
    });
}

/// Runs `f`, collecting the stylesheets used meanwhile. (Those of a render
/// within it are also an enclosing collection's.)
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn collect<R>(f: impl FnOnce() -> R) -> (R, RenderedCss) {
    /// Restores the enclosing collection, also if `f` panics.
    struct Restore(Option<Vec<&'static Css>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let outer = self.0.take();
            let inner = RENDERING.replace(outer);
            if let Some(inner) = inner {
                note(&inner);
            }
        }
    }
    let restore = Restore(RENDERING.replace(Some(Vec::new())));
    let r = f();
    let css = RENDERING.take().unwrap_or_default();
    drop(restore);
    note(&css);
    (r, RenderedCss(css))
}
