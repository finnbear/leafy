//! leafy: a yew-like web UI library without a virtual DOM, optimized for
//! binary size.
//!
//! The API is a near drop-in replacement for [yew](https://yew.rs)'s:
//! function and struct components, `html!`, hooks, contexts, callbacks and
//! properties, under the same paths (e.g. [`prelude`]).
//!
//! `html!` compiles each element tree into a static template (HTML, parsed by
//! the browser once and cloned) and a list of the dynamic parts ("holes").
//! Rendering builds only the holes; patching compares them with the previous
//! render's. See the repository README for binary size results.
//!
//! # Example
//!
//! ```no_run
//! use leafy::prelude::*;
//!
//! #[function_component]
//! fn App() -> Html {
//!     let count = use_state(|| 0);
//!     let onclick = {
//!         let count = count.clone();
//!         Callback::from(move |_: MouseEvent| count.set(*count + 1))
//!     };
//!     html! {
//!         <button {onclick}>{"Clicked "}{*count}{" times"}</button>
//!     }
//! }
//!
//! fn main() {
//!     leafy::Renderer::<App>::new().render();
//! }
//! ```
//!
//! # Server-side rendering
//!
//! On non-wasm targets, `ServerRenderer` renders a component to an HTML
//! string, without a DOM: `ServerRenderer::<App>::new().render()`. Effects
//! don't run, and the output isn't hydratable. `render_with_css()` also
//! returns the stylesheets the output uses (of `css!` and `global_css!`),
//! and `render_page()` renders a whole page with them in its `<head>`.

mod attr_value;
mod callback;
mod children;
mod classes;
mod component;
mod context;
mod css;
mod dom;
mod hooks;
mod vdom;
extern crate self as leafy;
mod node_ref;
#[doc(hidden)]
pub mod props;
mod renderer;
#[cfg(not(target_arch = "wasm32"))]
mod ssr;
mod struct_component;

pub use attr_value::AttrValue;
pub use callback::Callback;
pub use children::{ChildProps, ChildrenRenderer, ChildrenWithProps, VChild};
pub use classes::Classes;
pub use component::{BaseComponent, Properties};
pub use context::{
    ContextHandle, ContextProvider, ContextProviderProps, use_context, use_context_selector,
};
#[cfg(not(target_arch = "wasm32"))]
pub use css::RenderedCss;
pub use css::{Css, CssClass};
pub use hooks::*;
pub use leafy_macro::{
    Properties, classes, css, function_component, function_component as component, global_css,
    hook, html, html_nested, props,
};
pub use node_ref::NodeRef;
pub use renderer::{AppHandle, Renderer, set_custom_panic_hook};
#[cfg(not(target_arch = "wasm32"))]
pub use ssr::ServerRenderer;
pub use struct_component::{Component, Context, Scope, SendAsMessage};
pub use vdom::{Children, Html, IntoPropValue, Key, NodeSeq, VNode};

#[doc(hidden)]
pub use wasm_bindgen;
#[doc(hidden)]
pub use web_sys;

use wasm_bindgen::JsCast;
use web_sys::{Event as WsEvent, EventTarget};

/// The result of rendering.
pub type HtmlResult = Result<Html, std::convert::Infallible>;

/// Paths as in yew's `yew::events` module.
pub mod events {
    pub use crate::TargetCast;
    pub use web_sys::{
        AnimationEvent, DragEvent, ErrorEvent, Event, FocusEvent, InputEvent, KeyboardEvent,
        MouseEvent, PointerEvent, ProgressEvent, SubmitEvent, TouchEvent, TransitionEvent, UiEvent,
        WheelEvent,
    };
}

/// Paths as in yew's `yew::virtual_dom` module.
pub mod virtual_dom {
    pub use crate::VChild;
    pub use crate::vdom::{Html as VNode, Key, VListBuilder as VList};
    pub use crate::{AttrValue, Classes};
}

/// Paths as in yew's `yew::html` module.
pub mod html {
    pub use crate::vdom::{Children, Html, IntoPropValue};
    pub use crate::{
        BaseComponent, ChildrenRenderer, ChildrenWithProps, Classes, Component, Context, NodeRef,
        Properties, Scope, TargetCast,
    };
}

/// Paths as in yew's `yew::functional` module.
pub mod functional {
    pub use crate::hooks::*;
    pub use crate::{function_component, hook, use_context};
}

/// Paths as in yew's `yew::platform` module.
pub mod platform {
    /// Runs a future on the current thread.
    pub fn spawn_local<F: std::future::Future<Output = ()> + 'static>(f: F) {
        wasm_bindgen_futures::spawn_local(f);
    }
}

pub use events::*;

/// The common imports, as yew's `yew::prelude`.
pub mod prelude {
    pub use crate::events::*;
    pub use crate::hooks::*;
    pub use crate::{
        AttrValue, BaseComponent, Callback, Children, ChildrenWithProps, Classes, Component,
        Context, ContextProvider, Html, HtmlResult, NodeRef, Properties, classes, component, css,
        function_component, global_css, hook, html, html_nested, use_context, use_context_selector,
    };
}

/// Casts the target of an event.
pub trait TargetCast: AsRef<WsEvent> {
    /// The event's target, if it is a `T`.
    fn target_dyn_into<T: AsRef<EventTarget> + JsCast>(&self) -> Option<T> {
        self.as_ref().target().and_then(|t| t.dyn_into().ok())
    }

    /// The event's target, cast to `T` without checking.
    ///
    /// Panics if the event has no target.
    fn target_unchecked_into<T: AsRef<EventTarget> + JsCast>(&self) -> T {
        self.as_ref().target().unwrap().unchecked_into()
    }
}

impl<E: AsRef<WsEvent>> TargetCast for E {}

/// Converts the value of an `on*={..}` attribute.
pub trait IntoEventCallback<E> {
    /// The callback, or `None` for no listener.
    fn into_event_callback(self) -> Option<Callback<E>>;
}

impl<E> IntoEventCallback<E> for Callback<E> {
    #[inline]
    fn into_event_callback(self) -> Option<Callback<E>> {
        Some(self)
    }
}

impl<E> IntoEventCallback<E> for &Callback<E> {
    #[inline]
    fn into_event_callback(self) -> Option<Callback<E>> {
        Some(self.clone())
    }
}

impl<E> IntoEventCallback<E> for Option<Callback<E>> {
    #[inline]
    fn into_event_callback(self) -> Option<Callback<E>> {
        self
    }
}

impl<E> IntoEventCallback<E> for &Option<Callback<E>> {
    #[inline]
    fn into_event_callback(self) -> Option<Callback<E>> {
        self.clone()
    }
}

impl<E, F: Fn(E) + 'static> IntoEventCallback<E> for Option<F> {
    #[inline]
    fn into_event_callback(self) -> Option<Callback<E>> {
        self.map(Callback::from)
    }
}

impl<E, F: Fn(E) + 'static> IntoEventCallback<E> for F {
    #[inline]
    fn into_event_callback(self) -> Option<Callback<E>> {
        Some(Callback::from(self))
    }
}

// ----------------------------------------------- `html!` expansion helpers

#[doc(hidden)]
pub use component::{__comp, __comp_b, Build as __Build, CompState as __CompState};
#[doc(hidden)]
pub use props::{IsSet as __IsSet, Set as __Set};
#[doc(hidden)]
#[cfg(not(target_arch = "wasm32"))]
pub use vdom::SsrHole as __SsrHole;
#[doc(hidden)]
pub use vdom::{__children_extend, __children_push, Dyn as __Dyn, Tpl as __Tpl};

thread_local! {
    /// Holes of the templates being built, innermost last. (A stack shared
    /// by all templates saves each one a vector, and each hole an argument.)
    static HOLES: std::cell::UnsafeCell<Vec<vdom::Dyn>> = const { std::cell::UnsafeCell::new(Vec::new()) };
}

/// The holes of a template being built: those above `base`. If dropped (a
/// hole's expression returned early), discards them.
#[doc(hidden)]
pub struct __Frame(usize);

impl Drop for __Frame {
    #[inline(never)]
    fn drop(&mut self) {
        // One at a time, each dropped with no reference to the stack live:
        // dropping a hole can run user code, which can use `html!`.
        // SAFETY: no reference to the stack outlives a call into this module.
        while let Some(d) = HOLES.with(|h| unsafe {
            let v = &mut *h.get();
            if v.len() > self.0 { v.pop() } else { None }
        }) {
            drop(d);
        }
    }
}

#[doc(hidden)]
#[inline(never)]
pub fn __begin() -> __Frame {
    // SAFETY: as above.
    __Frame(HOLES.with(|h| unsafe { (*h.get()).len() }))
}

/// Takes the holes of a template.
#[inline(never)]
fn end_frame(f: __Frame) -> Vec<vdom::Dyn> {
    let base = f.0;
    std::mem::forget(f);
    // SAFETY: as above.
    HOLES.with(|h| unsafe { (*h.get()).split_off(base) })
}

#[inline(never)]
fn push_hole(d: vdom::Dyn) {
    // SAFETY: as above.
    HOLES.with(|h| unsafe { (*h.get()).push(d) });
}

#[doc(hidden)]
#[inline(never)]
pub fn __end(tpl: &'static vdom::Tpl, f: __Frame) -> Html {
    Html::__tpl(tpl, None, end_frame(f))
}

#[doc(hidden)]
#[inline(never)]
pub fn __end_keyed(tpl: &'static vdom::Tpl, key: Key, f: __Frame) -> Html {
    Html::__tpl(tpl, Some(key), end_frame(f))
}

#[doc(hidden)]
#[inline(never)]
pub fn __attr<V: IntoPropValue<Option<AttrValue>>>(v: V) {
    push_hole(vdom::Dyn::Attr(v.into_prop_value()))
}

#[doc(hidden)]
#[inline(never)]
pub fn __bool(v: bool) {
    push_hole(vdom::Dyn::Attr(v.then_some(AttrValue::Static(""))))
}

#[doc(hidden)]
#[inline(never)]
pub fn __class<V: Into<Classes>>(v: V) {
    let classes = v.into();
    // On the server, `css!` classes keep their stylesheets, for the render.
    #[cfg(not(target_arch = "wasm32"))]
    if classes.css.is_some() {
        return push_hole(vdom::Dyn::Class(Box::new(classes)));
    }
    push_hole(vdom::Dyn::Attr(classes.into_attr()))
}

#[doc(hidden)]
#[inline(never)]
pub fn __on<E: JsCast + 'static, V: IntoEventCallback<E>>(v: V) {
    push_hole(
        // SAFETY: web-sys event types are `#[repr(transparent)]` wrappers
        // of `Event`, so ABI-compatible with it.
        vdom::Dyn::Listener(
            v.into_event_callback()
                .map(|cb| unsafe { cb.__cast::<WsEvent>() }),
        ),
    )
}

#[doc(hidden)]
#[inline(never)]
pub fn __ref<V: IntoPropValue<NodeRef>>(v: V) {
    push_hole(vdom::Dyn::Ref(v.into_prop_value()))
}

#[doc(hidden)]
#[inline(never)]
pub fn __child<IN>(v: NodeSeq<IN>) {
    push_hole(vdom::Dyn::Child(v.0))
}

#[doc(hidden)]
#[inline(never)]
pub fn __child_html(v: Html) {
    push_hole(vdom::Dyn::Child(v))
}

#[doc(hidden)]
#[inline(never)]
pub fn __html<IN>(v: NodeSeq<IN>) -> Html {
    v.0
}

/// Autoref specialization of `{expr}`: some types convert to [`Html`] more
/// cheaply than through `ToString`. `(&w).__kind()` picks a tag (by which
/// trait applies), which then converts `w`: `html!` writes
/// `match __W(expr) { w => (&w).__kind().__push(w) }`.
#[doc(hidden)]
pub struct __W<T>(pub T);

#[doc(hidden)]
pub struct __TString;
#[doc(hidden)]
pub struct __TAttr;
#[doc(hidden)]
pub struct __THtml;
#[doc(hidden)]
pub struct __TAny;

#[doc(hidden)]
pub trait __SpecStr {
    type Tag;
    fn __kind(&self) -> Self::Tag;
}

impl __SpecStr for __W<String> {
    type Tag = __TString;
    #[inline(always)]
    fn __kind(&self) -> __TString {
        __TString
    }
}

impl __SpecStr for __W<AttrValue> {
    type Tag = __TAttr;
    #[inline(always)]
    fn __kind(&self) -> __TAttr {
        __TAttr
    }
}

impl __SpecStr for __W<Html> {
    type Tag = __THtml;
    #[inline(always)]
    fn __kind(&self) -> __THtml {
        __THtml
    }
}

#[doc(hidden)]
pub trait __SpecAny {
    fn __kind(&self) -> __TAny;
}

impl<T> __SpecAny for &__W<T> {
    #[inline(always)]
    fn __kind(&self) -> __TAny {
        __TAny
    }
}

impl __TString {
    #[inline(always)]
    pub fn __push(self, w: __W<String>) {
        __child_html(Html::__text(w.0))
    }
    #[inline(always)]
    pub fn __to_html(self, w: __W<String>) -> Html {
        Html::__text(w.0)
    }
}

impl __TAttr {
    #[inline(always)]
    pub fn __push(self, w: __W<AttrValue>) {
        __child_html(Html::Text(w.0))
    }
    #[inline(always)]
    pub fn __to_html(self, w: __W<AttrValue>) -> Html {
        Html::Text(w.0)
    }
}

impl __THtml {
    #[inline(always)]
    pub fn __push(self, w: __W<Html>) {
        __child_html(w.0)
    }
    #[inline(always)]
    pub fn __to_html(self, w: __W<Html>) -> Html {
        w.0
    }
}

impl __TAny {
    #[inline(always)]
    pub fn __push<T: Into<NodeSeq<IN>>, IN>(self, w: __W<T>) {
        __child(w.0.into())
    }
    #[inline(always)]
    pub fn __to_html<T: Into<NodeSeq<IN>>, IN>(self, w: __W<T>) -> Html {
        __html(w.0.into())
    }
}

/// A required property was unset (impossible through the builder's typed
/// setters).
#[doc(hidden)]
#[cold]
#[inline(never)]
pub fn __missing_prop() -> ! {
    panic!("missing required property")
}

#[doc(hidden)]
#[inline(never)]
pub fn __key<K: Into<Key>>(k: K) -> Key {
    k.into()
}

#[doc(hidden)]
#[cfg(target_arch = "wasm32")]
pub const fn __tpl_new(html: &'static str, walk: &'static str) -> vdom::Tpl {
    vdom::Tpl {
        html,
        walk,
        id: std::sync::atomic::AtomicU32::new(0),
    }
}

#[doc(hidden)]
#[cfg(not(target_arch = "wasm32"))]
pub const fn __tpl_new_ssr(
    html: &'static str,
    walk: &'static str,
    ssr: &'static [vdom::SsrHole],
) -> vdom::Tpl {
    vdom::Tpl {
        html,
        walk,
        id: std::sync::atomic::AtomicU32::new(0),
        ssr,
    }
}
