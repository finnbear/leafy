//! The value a component renders: a tree of static templates whose dynamic
//! parts ("holes") are the only things built, compared and patched at runtime.

use crate::component::VComp;
use crate::{AttrValue, NodeRef};
use std::borrow::Cow;
use std::fmt;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::atomic::AtomicU32;
use web_sys::Event;

/// A static template, emitted by `html!`.
///
/// `html` is parsed by the browser once (as the contents of a `<template>`),
/// and each instance is a deep clone of it. `walk` is a tiny program, run on
/// each clone, which finds the nodes that holes refer to (see `dom.rs`).
pub struct Tpl {
    /// The template's static HTML.
    pub html: &'static str,
    /// The program that finds the nodes holes refer to.
    pub walk: &'static str,
    /// 1 + index into the cache of parsed templates, or 0 if not yet parsed.
    pub id: AtomicU32,
    /// For server rendering: each hole's.
    #[cfg(not(target_arch = "wasm32"))]
    pub ssr: &'static [SsrHole],
}

/// For server rendering, a hole of a template: where in its HTML (without
/// comments) the hole's output goes, its kind (`a`: attribute named by the
/// third field, `v`: a textarea's value, `c`: child, `s`: child written as
/// is if text, `-`: nothing), and its attribute's name.
#[doc(hidden)]
#[cfg(not(target_arch = "wasm32"))]
pub struct SsrHole(pub u32, pub u8, pub &'static str);

/// A rendered value, cheap to clone.
#[derive(Default)]
pub enum Html {
    /// Nothing.
    #[default]
    Empty,
    /// A text node.
    Text(AttrValue),
    /// An instance of an `html!` template, with the values of its holes.
    Tpl(Rc<VTpl>),
    /// A fragment: a list of nodes, which may be keyed.
    List(Rc<VList>),
    /// A component, with its key and properties.
    Comp(Rc<VComp>),
    /// Raw HTML, parsed in the context of its parent element.
    Raw(AttrValue),
    /// An existing DOM node.
    VRef(web_sys::Node),
}

/// An alias, for compatibility.
pub type VNode = Html;
/// The `children` of a component: [`Html`], which can also be iterated.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Children(pub Html);

impl Children {
    /// The top-level nodes.
    pub fn iter(&self) -> impl Iterator<Item = Html> + '_ {
        let (single, list) = match &self.0 {
            Html::Empty => (None, &[][..]),
            Html::List(l) if l.key.is_none() => (None, &l.children[..]),
            html => (Some(html.clone()), &[][..]),
        };
        single.into_iter().chain(list.iter().cloned())
    }

    /// Whether there are no children.
    pub fn is_empty(&self) -> bool {
        match &self.0 {
            Html::Empty => true,
            Html::List(l) => l.children.is_empty(),
            _ => false,
        }
    }

    /// The number of top-level nodes.
    pub fn len(&self) -> usize {
        self.iter().count()
    }
}

impl<'a> IntoIterator for &'a Children {
    type Item = Html;
    type IntoIter = Box<dyn Iterator<Item = Html> + 'a>;

    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.iter())
    }
}

impl From<Children> for Html {
    #[inline]
    fn from(c: Children) -> Self {
        c.0
    }
}

impl From<&Children> for Html {
    #[inline]
    fn from(c: &Children) -> Self {
        c.0.clone()
    }
}

impl IntoPropValue<Children> for Html {
    #[inline]
    fn into_prop_value(self) -> Children {
        Children(self)
    }
}

/// For a component whose `children` are optional (`Option<Children>`): given,
/// they're `Some`.
impl IntoPropValue<Option<Children>> for Html {
    #[inline]
    fn into_prop_value(self) -> Option<Children> {
        Some(Children(self))
    }
}

impl IntoPropValue<Html> for Children {
    #[inline]
    fn into_prop_value(self) -> Html {
        self.0
    }
}

pub struct VTpl {
    pub(crate) tpl: &'static Tpl,
    pub(crate) key: Option<Key>,
    pub(crate) dyns: Vec<Dyn>,
}

pub struct VList {
    pub(crate) children: Vec<Html>,
    pub(crate) key: Option<Key>,
}

impl VList {
    pub fn children(&self) -> &[Html] {
        &self.children
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Html> {
        self.children.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }

    pub fn len(&self) -> usize {
        self.children.len()
    }
}

/// The value of one hole of a template.
pub enum Dyn {
    /// An attribute's value, or `None` for no attribute.
    Attr(Option<AttrValue>),
    /// An event listener, or `None` for none.
    Listener(Option<crate::Callback<Event>>),
    /// A `ref={..}`.
    Ref(NodeRef),
    /// A child.
    Child(Html),
    /// On the server, a `class` with [`css!`](crate::css) classes, whose
    /// stylesheets a render of it uses.
    #[cfg(not(target_arch = "wasm32"))]
    Class(Box<crate::Classes>),
}

/// A key, identifying an item in a list.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Key {
    /// An integer key (also made from a `char`).
    Int(i64),
    /// A string key.
    Str(AttrValue),
}

macro_rules! key_int {
    ($($t:ty)*) => {$(
        impl From<$t> for Key {
            #[inline]
            fn from(v: $t) -> Self {
                Key::Int(v as i64)
            }
        }
    )*};
}
key_int!(u8 u16 u32 u64 usize i8 i16 i32 i64 isize);

impl From<&'static str> for Key {
    fn from(v: &'static str) -> Self {
        Key::Str(AttrValue::Static(v))
    }
}
impl From<String> for Key {
    fn from(v: String) -> Self {
        Key::Str(v.into())
    }
}
impl From<AttrValue> for Key {
    fn from(v: AttrValue) -> Self {
        Key::Str(v)
    }
}
impl From<Rc<str>> for Key {
    fn from(v: Rc<str>) -> Self {
        Key::Str(v.into())
    }
}
impl From<char> for Key {
    fn from(v: char) -> Self {
        Key::Int(v as i64)
    }
}

impl Html {
    #[doc(hidden)]
    #[inline(never)]
    pub fn __text(s: String) -> Self {
        Html::Text(AttrValue::from(s))
    }

    #[doc(hidden)]
    #[inline(never)]
    pub fn __tpl(tpl: &'static Tpl, key: Option<Key>, dyns: Vec<Dyn>) -> Self {
        Html::Tpl(Rc::new(VTpl { tpl, key, dyns }))
    }

    #[doc(hidden)]
    #[inline(never)]
    pub fn __tpl0(tpl: &'static Tpl, dyns: Vec<Dyn>) -> Self {
        Self::__tpl(tpl, None, dyns)
    }

    pub(crate) fn key(&self) -> Option<&Key> {
        match self {
            Html::Tpl(t) => t.key.as_ref(),
            Html::Comp(c) => c.key.as_ref(),
            Html::List(l) => l.key.as_ref(),
            _ => None,
        }
    }

    /// Raw HTML, which is not escaped (so must be trusted).
    pub fn from_html_unchecked(html: AttrValue) -> Self {
        Html::Raw(html)
    }

    /// Whether this renders nothing.
    pub fn is_empty(&self) -> bool {
        matches!(self, Html::Empty)
    }

    /// A keyed fragment, `<key={k}>..</>`.
    #[doc(hidden)]
    #[inline(never)]
    pub fn __keyed(key: Key, html: Html) -> Self {
        Html::List(Rc::new(VList {
            children: vec![html],
            key: Some(key),
        }))
    }

    #[doc(hidden)]
    pub fn __from_vec(children: Vec<Html>) -> Self {
        if children.is_empty() {
            Html::Empty
        } else {
            Html::List(Rc::new(VList {
                children,
                key: None,
            }))
        }
    }
}

impl Clone for Html {
    #[inline(never)]
    fn clone(&self) -> Self {
        match self {
            Html::Empty => Html::Empty,
            Html::Text(t) => Html::Text(t.clone()),
            Html::Tpl(t) => Html::Tpl(t.clone()),
            Html::List(l) => Html::List(l.clone()),
            Html::Comp(c) => Html::Comp(c.clone()),
            Html::Raw(r) => Html::Raw(r.clone()),
            Html::VRef(n) => Html::VRef(n.clone()),
        }
    }
}

impl PartialEq for Html {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Html::Empty, Html::Empty) => true,
            (Html::Text(a), Html::Text(b)) | (Html::Raw(a), Html::Raw(b)) => a == b,
            (Html::VRef(a), Html::VRef(b)) => a == b,
            (Html::Tpl(a), Html::Tpl(b)) => {
                Rc::ptr_eq(a, b)
                    || (std::ptr::eq(a.tpl, b.tpl) && a.key == b.key && a.dyns == b.dyns)
            }
            (Html::List(a), Html::List(b)) => {
                Rc::ptr_eq(a, b) || (a.key == b.key && a.children == b.children)
            }
            (Html::Comp(a), Html::Comp(b)) => {
                Rc::ptr_eq(a, b) || (a.key == b.key && a.same_type(b) && a.props.props_eq(&b.props))
            }
            _ => false,
        }
    }
}

impl PartialEq for Dyn {
    #[allow(ambiguous_wide_pointer_comparisons)]
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Dyn::Attr(a), Dyn::Attr(b)) => a == b,
            #[cfg(not(target_arch = "wasm32"))]
            (Dyn::Class(a), Dyn::Class(b)) => a == b,
            #[cfg(not(target_arch = "wasm32"))]
            (Dyn::Attr(a), Dyn::Class(b)) | (Dyn::Class(b), Dyn::Attr(a)) => {
                a.as_deref() == (!b.is_empty()).then(|| b.as_str())
            }
            (Dyn::Listener(a), Dyn::Listener(b)) => a == b,
            (Dyn::Ref(a), Dyn::Ref(b)) => a == b,
            (Dyn::Child(a), Dyn::Child(b)) => a == b,
            _ => false,
        }
    }
}

impl fmt::Debug for Html {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Html::Empty => f.write_str("Empty"),
            Html::Text(t) => write!(f, "Text({t:?})"),
            Html::Tpl(t) => write!(f, "Tpl({:?})", t.tpl.html),
            Html::List(l) => f.debug_list().entries(l.children.iter()).finish(),
            Html::Comp(_) => f.write_str("Comp"),
            Html::Raw(r) => write!(f, "Raw({r:?})"),
            Html::VRef(_) => f.write_str("VRef"),
        }
    }
}

impl<T: ToString> From<T> for Html {
    #[inline]
    fn from(t: T) -> Self {
        Html::__text(t.to_string())
    }
}

impl<A: Into<Html>> FromIterator<A> for Html {
    #[inline]
    fn from_iter<I: IntoIterator<Item = A>>(iter: I) -> Self {
        // A plain loop around non-generic code, rather than `Vec`'s
        // specialized collection, which is a lot of code per iterator type.
        let mut v = Vec::new();
        for a in iter {
            push_html(&mut v, a.into());
        }
        Html::__from_vec(v)
    }
}

#[inline(never)]
fn push_html(v: &mut Vec<Html>, h: Html) {
    v.push(h);
}

/// Adds a component's child (see `html!`).
#[doc(hidden)]
#[inline(never)]
pub fn __children_push(v: &mut Vec<Html>, h: Html) {
    v.push(h);
}

/// Adds the items of a `for` among a component's children.
#[doc(hidden)]
#[inline(never)]
pub fn __children_extend(v: &mut Vec<Html>, h: Html) {
    match h {
        Html::Empty => {}
        Html::List(l) if l.key.is_none() => v.extend(l.children.iter().cloned()),
        h => v.push(h),
    }
}

/// Converts the value of `{expr}` in `html!` to [`Html`], accepting anything
/// `Into<Html>`, an `Option` or a `Vec` of such.
pub struct NodeSeq<IN>(pub(crate) Html, PhantomData<IN>);

impl<IN: Into<Html>> From<IN> for NodeSeq<IN> {
    #[inline]
    fn from(v: IN) -> Self {
        Self(v.into(), PhantomData)
    }
}

impl<IN: Into<Html>> From<Option<IN>> for NodeSeq<IN> {
    #[inline]
    fn from(v: Option<IN>) -> Self {
        Self(v.map(Into::into).unwrap_or_default(), PhantomData)
    }
}

impl<IN: Into<Html>> From<Vec<IN>> for NodeSeq<IN> {
    #[inline]
    fn from(v: Vec<IN>) -> Self {
        Self(v.into_iter().collect(), PhantomData)
    }
}

/// Conversion of property values, as yew's.
pub trait IntoPropValue<T> {
    /// Converts to the property's type.
    fn into_prop_value(self) -> T;
}

impl<T> IntoPropValue<T> for T {
    #[inline]
    fn into_prop_value(self) -> T {
        self
    }
}

impl<T> IntoPropValue<Option<T>> for T {
    #[inline]
    fn into_prop_value(self) -> Option<T> {
        Some(self)
    }
}

impl<T: Clone> IntoPropValue<T> for &T {
    #[inline]
    fn into_prop_value(self) -> T {
        self.clone()
    }
}

impl<T: Clone> IntoPropValue<Option<T>> for &T {
    #[inline]
    fn into_prop_value(self) -> Option<T> {
        Some(self.clone())
    }
}

impl<I, O, F: Fn(I) -> O + 'static> IntoPropValue<crate::Callback<I, O>> for F {
    #[inline]
    fn into_prop_value(self) -> crate::Callback<I, O> {
        crate::Callback::from(self)
    }
}

impl<I, O, F: Fn(I) -> O + 'static> IntoPropValue<Option<crate::Callback<I, O>>> for F {
    #[inline]
    fn into_prop_value(self) -> Option<crate::Callback<I, O>> {
        Some(crate::Callback::from(self))
    }
}

impl<I, O, F: Fn(I) -> O + 'static> IntoPropValue<Option<crate::Callback<I, O>>> for Option<F> {
    #[inline]
    fn into_prop_value(self) -> Option<crate::Callback<I, O>> {
        self.map(crate::Callback::from)
    }
}

macro_rules! impl_into_prop {
    (|$value:ident: $from_ty:ty| -> $to_ty:ty { $conversion:expr }) => {
        impl IntoPropValue<$to_ty> for $from_ty {
            #[inline]
            fn into_prop_value(self) -> $to_ty {
                let $value = self;
                $conversion
            }
        }
        impl IntoPropValue<Option<$to_ty>> for $from_ty {
            #[inline]
            fn into_prop_value(self) -> Option<$to_ty> {
                let $value = self;
                Some({ $conversion })
            }
        }
        impl IntoPropValue<Option<$to_ty>> for Option<$from_ty> {
            #[inline]
            fn into_prop_value(self) -> Option<$to_ty> {
                self.map(IntoPropValue::into_prop_value)
            }
        }
    };
}

impl_into_prop!(|value: &'static str| -> String { value.to_owned() });
impl_into_prop!(|value: &'static str| -> AttrValue { AttrValue::Static(value) });
impl_into_prop!(|value: String| -> AttrValue { AttrValue::from(value) });
impl_into_prop!(|value: Rc<str>| -> AttrValue { AttrValue::Rc(value) });
impl_into_prop!(|value: Cow<'static, str>| -> AttrValue { AttrValue::from(value) });
impl_into_prop!(|value: &'static str| -> Cow<'static, str> { Cow::Borrowed(value) });
impl_into_prop!(|value: String| -> Cow<'static, str> { Cow::Owned(value) });
impl_into_prop!(|value: &'static str| -> crate::Classes { crate::Classes::from(value) });
impl_into_prop!(|value: String| -> crate::Classes { crate::Classes::from(value) });
impl_into_prop!(|value: AttrValue| -> crate::Classes { crate::Classes::from(value) });

macro_rules! impl_into_html_prop {
    ($($t:ty)*) => {$(
        impl IntoPropValue<Html> for $t {
            #[inline]
            fn into_prop_value(self) -> Html {
                Html::from(self)
            }
        }
        impl IntoPropValue<Option<Html>> for $t {
            #[inline]
            fn into_prop_value(self) -> Option<Html> {
                Some(Html::from(self))
            }
        }
    )*};
}
impl_into_html_prop!(&'static str String AttrValue bool char u8 u16 u32 u64 usize i8 i16 i32 i64 isize f32 f64);

impl IntoPropValue<Html> for Vec<Html> {
    #[inline]
    fn into_prop_value(self) -> Html {
        Html::__from_vec(self)
    }
}

impl IntoPropValue<Html> for Option<Html> {
    #[inline]
    fn into_prop_value(self) -> Html {
        self.unwrap_or_default()
    }
}

/// A list of [`Html`], for building one imperatively (as yew's `VList`).
#[derive(Clone, Default, PartialEq)]
pub struct VListBuilder {
    /// The nodes.
    pub children: Vec<Html>,
    /// The list's key, if any.
    pub key: Option<Key>,
}

impl VListBuilder {
    /// An empty list.
    pub fn new() -> Self {
        Self::default()
    }

    /// A list of `children`, with `key`.
    pub fn with_children(children: Vec<Html>, key: Option<Key>) -> Self {
        Self { children, key }
    }

    /// Appends a node.
    pub fn push(&mut self, child: Html) {
        self.children.push(child);
    }

    /// Appends a node (as [`push`](Self::push)).
    pub fn add_child(&mut self, child: Html) {
        self.children.push(child);
    }

    /// Appends nodes.
    pub fn add_children(&mut self, children: impl IntoIterator<Item = Html>) {
        self.children.extend(children);
    }

    /// Whether there are no nodes.
    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }

    /// The number of nodes.
    pub fn len(&self) -> usize {
        self.children.len()
    }

    /// Iterates over the nodes.
    pub fn iter(&self) -> std::slice::Iter<'_, Html> {
        self.children.iter()
    }
}

impl From<Html> for VListBuilder {
    fn from(html: Html) -> Self {
        match html {
            Html::List(l) if l.key.is_none() => Self {
                children: l.children.clone(),
                key: None,
            },
            Html::Empty => Self::new(),
            html => Self {
                children: vec![html],
                key: None,
            },
        }
    }
}

impl From<VListBuilder> for Html {
    fn from(l: VListBuilder) -> Self {
        if l.children.is_empty() && l.key.is_none() {
            Html::Empty
        } else {
            Html::List(Rc::new(VList {
                children: l.children,
                key: l.key,
            }))
        }
    }
}

impl IntoPropValue<Html> for VListBuilder {
    fn into_prop_value(self) -> Html {
        self.into()
    }
}
