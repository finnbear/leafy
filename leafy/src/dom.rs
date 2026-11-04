//! Mounting and patching [`Html`] into the DOM.
//!
//! A template instance is a deep clone of a parsed `<template>`. Its `walk`
//! program finds the nodes the holes act on:
//!
//! | op        | meaning                                                         |
//! |-----------|-----------------------------------------------------------------|
//! | `f`       | descend to the first child                                      |
//! | `n`       | move to the next sibling                                        |
//! | `u`       | ascend to the parent                                            |
//! | `R`       | the current node is a root of the template                      |
//! | `a`name`;`| attribute hole on the current element                           |
//! | `p`name`;`| property hole (`value`, `checked`, ...) on the current element  |
//! | `e`name`;`| event listener hole on the current element (`E`: passive)       |
//! | `r`       | node ref hole on the current element                            |
//! | `c`       | child hole inserted before the current node                     |
//! | `C`       | child hole inserted before the current node, and a root         |
//! | `l`       | child hole appended to the current element                      |
//! | `W`       | (first) the template is parsed inside a wrapper `<svg>`/`<math>`|
//! | `A`       | (first) the template is parsed inside a wrapper `<svg>` if it's |
//! |           | mounted in SVG (its elements are SVG's too, e.g. `<a>`)         |

use crate::component::CompState;
use crate::vdom::{Dyn, Html, VList, VTpl};
use crate::{AttrValue, NodeRef};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::Event;

#[wasm_bindgen]
extern "C" {
    /// A DOM node, with just the methods leafy uses (and no exception
    /// handling, which would cost code size).
    #[derive(Clone, PartialEq)]
    pub(crate) type Node;
    pub(crate) type Doc;

    #[wasm_bindgen(thread_local_v2, js_name = document)]
    static DOCUMENT: Doc;

    #[wasm_bindgen(js_name = queueMicrotask)]
    fn queue_microtask_js(f: &JsValue);

    #[wasm_bindgen(method, js_name = createElement)]
    fn create_element(this: &Doc, tag: &str) -> Node;
    #[wasm_bindgen(method, js_name = createTextNode)]
    fn create_text(this: &Doc, data: &str) -> Node;
    #[wasm_bindgen(method, js_name = importNode)]
    fn import_node(this: &Doc, node: &JsValue, deep: bool) -> Node;
    #[wasm_bindgen(method, getter)]
    fn body(this: &Doc) -> Node;
    #[wasm_bindgen(method, getter)]
    fn head(this: &Doc) -> Node;

    #[wasm_bindgen(method, setter = innerHTML)]
    fn set_inner_html(this: &Node, html: &str);
    #[wasm_bindgen(method, getter = content)]
    fn content(this: &Node) -> JsValue;

    #[wasm_bindgen(method, getter = firstChild)]
    fn first_child(this: &Node) -> Node;
    #[wasm_bindgen(method, getter = firstChild)]
    fn first_child_opt(this: &Node) -> Option<Node>;
    #[wasm_bindgen(method, getter = nextSibling)]
    fn next_sibling(this: &Node) -> Option<Node>;
    #[wasm_bindgen(method, getter = parentNode)]
    fn parent_node(this: &Node) -> Node;
    #[wasm_bindgen(method, js_name = insertBefore)]
    fn insert_before(this: &Node, node: &Node, before: Option<&Node>);
    #[wasm_bindgen(method, js_name = remove)]
    fn remove_js(this: &Node);
    #[wasm_bindgen(method, getter = nodeType)]
    fn node_type(this: &Node) -> u32;
    #[wasm_bindgen(method, js_name = cloneNode)]
    fn clone_node(this: &Node, deep: bool) -> Node;
    #[wasm_bindgen(method, setter = textContent)]
    fn set_text_content(this: &Node, text: &str);
    #[wasm_bindgen(method, setter = data)]
    fn set_data(this: &Node, data: &str);

    #[wasm_bindgen(method, js_name = setAttribute)]
    fn set_attribute(this: &Node, name: &str, value: &str);
    #[wasm_bindgen(method, js_name = removeAttribute)]
    fn remove_attribute(this: &Node, name: &str);
    #[wasm_bindgen(method, getter = value)]
    fn value(this: &Node) -> String;
    #[wasm_bindgen(method, setter = value)]
    fn set_value(this: &Node, value: &str);
    // (Each its own, rather than `Reflect.set` with the name: passing a
    // string from wasm costs a decode on every patch.)
    #[wasm_bindgen(method, getter = checked)]
    fn checked(this: &Node) -> bool;
    #[wasm_bindgen(method, setter = checked)]
    fn set_checked(this: &Node, value: bool);
    #[wasm_bindgen(method, setter = selected)]
    fn set_selected(this: &Node, value: bool);
    #[wasm_bindgen(method, setter = muted)]
    fn set_muted(this: &Node, value: bool);

    #[wasm_bindgen(method, js_name = addEventListener)]
    fn add_event_listener(this: &Node, name: &str, f: &JsValue, options: &JsValue);
    #[wasm_bindgen(method, js_name = removeEventListener)]
    fn remove_event_listener(this: &Node, name: &str, f: &JsValue);

    #[wasm_bindgen(js_name = SVGElement)]
    type SvgElement;
    #[wasm_bindgen(js_name = SVGForeignObjectElement)]
    type SvgForeignObjectElement;
    #[wasm_bindgen(js_name = SVGTitleElement)]
    type SvgTitleElement;
    #[wasm_bindgen(js_name = SVGDescElement)]
    type SvgDescElement;
}

thread_local! {
    /// Parsed templates (a template whose elements are SVG's as well as
    /// HTML's has two slots: parsed as HTML, and as SVG).
    static TEMPLATES: RefCell<Vec<Option<JsValue>>> = const { RefCell::new(Vec::new()) };
    /// Mounted [`Html::VRef`] nodes, and how many times each is mounted.
    static VREFS: RefCell<Vec<(Node, u32)>> = const { RefCell::new(Vec::new()) };
    /// The templates whose top-level holes are being mounted, innermost
    /// last: where each goes, and its fragment (not yet inserted, which they
    /// are mounted in). To find the element that content mounted in a
    /// fragment will be in.
    static OUTER: RefCell<Vec<(Pos<'static>, &'static Node)>> = const { RefCell::new(Vec::new()) };
    static FLUSH: Closure<dyn Fn()> = Closure::new(crate::component::flush);
    static PASSIVE: JsValue = {
        let o = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&o, &"passive".into(), &JsValue::TRUE);
        o.into()
    };
}

pub(crate) fn queue_microtask() {
    FLUSH.with(|f| queue_microtask_js(f.as_ref()));
}

pub(crate) fn body() -> Node {
    DOCUMENT.with(|d| d.body())
}

/// Appends a `<style>` with `css` to `<head>`.
#[cfg(target_arch = "wasm32")]
pub(crate) fn add_style(css: &str) {
    DOCUMENT.with(|d| {
        let style = d.create_element("style");
        style.set_text_content(css);
        d.head().insert_before(&style, None);
    });
}

pub(crate) fn empty_text() -> Node {
    DOCUMENT.with(|d| d.create_text(""))
}

pub(crate) fn clear(node: &Node) {
    node.set_text_content("");
}

pub(crate) fn remove(node: &Node) {
    node.remove_js();
}

/// Where to insert: before `before`, in `parent` (which, if `None`, is
/// `before`'s parent).
#[derive(Clone, Copy)]
pub(crate) struct Pos<'a> {
    pub(crate) parent: Option<&'a Node>,
    pub(crate) before: Option<&'a Node>,
    /// Whether the position stays valid for the life of what is mounted at
    /// it (so a component there needs no anchor of its own).
    pub(crate) stable: bool,
}

impl<'a> Pos<'a> {
    pub(crate) fn stable(parent: Option<&'a Node>, before: Option<&'a Node>) -> Self {
        Self {
            parent,
            before,
            stable: true,
        }
    }

    pub(crate) fn append(parent: &'a Node) -> Self {
        Self::stable(Some(parent), None)
    }

    fn unstable(parent: &'a Node, before: Option<&'a Node>) -> Self {
        Self {
            parent: Some(parent),
            before,
            stable: false,
        }
    }

    fn parent(&self) -> Node {
        match self.parent {
            Some(p) => p.clone(),
            None => self.before.unwrap().parent_node(),
        }
    }
}

pub(crate) fn insert(pos: Pos<'_>, node: &Node) {
    match pos.parent {
        Some(p) => p.insert_before(node, pos.before),
        None => pos.parent().insert_before(node, pos.before),
    }
}

/// Mounted [`Html`]: the DOM nodes and component instances it made.
#[derive(Default)]
pub(crate) enum M {
    #[default]
    Empty,
    Text(Node),
    Tpl(Box<MTpl>),
    List(Vec<M>),
    Comp(Rc<CompState>),
    Raw(Vec<Node>),
    /// An existing node ([`Html::VRef`]).
    VRef(Node),
}

pub(crate) struct MTpl {
    roots: Vec<Root>,
    holes: Vec<MHole>,
}

enum Root {
    Node(Node),
    Hole(usize),
}

enum MHole {
    Attr {
        el: Node,
        name: &'static str,
        prop: bool,
    },
    Listener(Listener),
    Ref(Node, NodeRef),
    Child {
        /// `None` for a hole at the top level of its template.
        parent: Option<Node>,
        /// `None` to append to `parent`.
        anchor: Option<Node>,
        m: M,
    },
}

type ListenerFn = crate::Callback<Event>;

struct Listener {
    el: Node,
    name: &'static str,
    passive: bool,
    f: Rc<RefCell<Option<ListenerFn>>>,
    closure: Option<Closure<dyn Fn(Event)>>,
}

impl Drop for Listener {
    fn drop(&mut self) {
        // Before the closure is freed: the element can still dispatch events
        // (e.g. `blur`, as it's removed, or one bubbling from a child whose
        // listener caused this).
        if let Some(c) = &self.closure {
            self.el.remove_event_listener(self.name, c.as_ref());
        }
    }
}

impl Listener {
    fn set(&mut self, f: &Option<ListenerFn>) {
        *self.f.borrow_mut() = f.clone();
        if f.is_some() && self.closure.is_none() {
            let slot = self.f.clone();
            let closure = Closure::<dyn Fn(Event)>::new(move |e: Event| {
                let f = slot.borrow().clone();
                if let Some(f) = f {
                    f.emit(e);
                }
            });
            if self.passive {
                PASSIVE.with(|o| self.el.add_event_listener(self.name, closure.as_ref(), o));
            } else {
                self.el
                    .add_event_listener(self.name, closure.as_ref(), &JsValue::UNDEFINED);
            }
            self.closure = Some(closure);
        }
    }
}

impl M {
    pub(crate) fn is_empty(&self) -> bool {
        matches!(self, M::Empty)
    }

    pub(crate) fn first_node(&self) -> Option<Node> {
        match self {
            M::Empty => None,
            M::Text(n) => Some(n.clone()),
            M::Tpl(t) => t.roots.iter().find_map(|r| match r {
                Root::Node(n) => Some(n.clone()),
                Root::Hole(i) => t.hole_m(*i).first_node(),
            }),
            M::List(l) => l.iter().find_map(M::first_node),
            M::Comp(c) => c.first_node(),
            M::Raw(r) => r.first().cloned(),
            M::VRef(n) => Some(n.clone()),
        }
    }

    pub(crate) fn move_before(&self, pos: Pos<'_>) {
        match self {
            M::Empty => {}
            M::Text(n) => insert(pos, n),
            M::Tpl(t) => {
                for r in &t.roots {
                    match r {
                        Root::Node(n) => insert(pos, n),
                        Root::Hole(i) => t.hole_m(*i).move_before(pos),
                    }
                }
            }
            M::List(l) => {
                for m in l {
                    m.move_before(pos);
                }
            }
            M::Comp(c) => c.move_before(pos),
            M::Raw(r) => {
                for n in r {
                    insert(pos, n);
                }
            }
            M::VRef(n) => insert(pos, n),
        }
    }

    /// Destroys components, and if `remove`, removes the nodes from the DOM.
    pub(crate) fn unmount(self, remove: bool) {
        match self {
            M::Empty => {}
            M::Text(n) => {
                if remove {
                    n.remove_js()
                }
            }
            M::Tpl(t) => {
                for h in t.holes {
                    match h {
                        MHole::Child { parent, m, .. } => m.unmount(remove && parent.is_none()),
                        MHole::Ref(el, r) => clear_ref(&r, &el),
                        _ => {}
                    }
                }
                if remove {
                    for r in t.roots {
                        if let Root::Node(n) = r {
                            n.remove_js();
                        }
                    }
                }
            }
            M::List(l) => {
                for m in l {
                    m.unmount(remove);
                }
            }
            M::Comp(c) => c.unmount(remove),
            M::Raw(r) => {
                if remove {
                    for n in r {
                        n.remove_js();
                    }
                }
            }
            M::VRef(n) => {
                // Unless it's also mounted elsewhere (where it may have just
                // moved).
                if vref_mounts(&n, false) == 0 {
                    n.remove_js();
                }
            }
        }
    }
}

/// Counts the mounts (`+1` if `add`, else `-1`) of a [`Html::VRef`] node,
/// returning how many remain.
#[inline(never)]
fn vref_mounts(node: &Node, add: bool) -> u32 {
    VREFS.with_borrow_mut(|v| {
        let i = v.iter().position(|(n, _)| n == node);
        match (i, add) {
            (Some(i), true) => {
                v[i].1 += 1;
                v[i].1
            }
            (None, true) => {
                v.push((node.clone(), 1));
                1
            }
            (Some(i), false) => {
                v[i].1 -= 1;
                let n = v[i].1;
                if n == 0 {
                    v.swap_remove(i);
                }
                n
            }
            (None, false) => 0,
        }
    })
}

impl MTpl {
    fn hole_m(&self, i: usize) -> &M {
        match &self.holes[i] {
            MHole::Child { m, .. } => m,
            _ => unreachable!(),
        }
    }
}

/// A fresh copy of a template's DOM (if `svg`, parsed in an `<svg>`).
fn instantiate(tpl: &'static crate::vdom::Tpl, svg: bool) -> Node {
    TEMPLATES.with_borrow_mut(|templates| {
        let mut id = tpl.id.load(Ordering::Relaxed) as usize;
        if id == 0 {
            templates.push(None);
            if tpl.walk.starts_with('A') {
                templates.push(None);
            }
            id = templates.len() - tpl.walk.starts_with('A') as usize;
            tpl.id.store(id as u32, Ordering::Relaxed);
        }
        let content = templates[id - 1 + svg as usize].get_or_insert_with(|| {
            let el = DOCUMENT.with(|d| d.create_element("template"));
            if svg {
                el.set_inner_html(&format!("<svg>{}", tpl.html));
            } else {
                el.set_inner_html(tpl.html);
            }
            let content = el.content();
            if svg || tpl.walk.starts_with('W') {
                // Parsed inside an `<svg>` or `<math>`: unwrap.
                let frag: &Node = content.unchecked_ref();
                let wrapper = frag.first_child();
                while let Some(n) = wrapper.first_child_opt() {
                    frag.insert_before(&n, None);
                }
                wrapper.remove_js();
            }
            content
        });
        DOCUMENT.with(|d| d.import_node(content, true))
    })
}

/// The element that content mounted in `parent` is in: `parent`, or (if
/// it's a template's fragment, not yet inserted) where it will be, if known.
fn context(mut parent: Node) -> Option<Node> {
    OUTER.with_borrow(|outer| {
        let mut outer = outer.iter().rev();
        while parent.node_type() != 1 {
            // A template's fragment (or another, e.g. a shadow root, whose
            // content is HTML).
            let (pos, frag) = outer.next()?;
            if parent != **frag {
                return None;
            }
            parent = pos.parent();
        }
        Some(parent)
    })
}

/// Whether content in `el` is SVG (not in one of the elements whose
/// content the HTML parser parses as HTML).
fn is_svg(el: &Node) -> bool {
    el.is_instance_of::<SvgElement>()
        && !el.is_instance_of::<SvgForeignObjectElement>()
        && !el.is_instance_of::<SvgTitleElement>()
        && !el.is_instance_of::<SvgDescElement>()
}

/// Pops [`OUTER`], if it pushed.
struct Outer(bool);

impl Drop for Outer {
    fn drop(&mut self) {
        if self.0 {
            OUTER.with_borrow_mut(|o| o.pop());
        }
    }
}

fn set_attr(el: &Node, name: &str, prop: bool, value: &Option<AttrValue>) {
    if prop {
        let v = value.is_some();
        match name {
            "checked" => {
                if el.checked() != v {
                    el.set_checked(v);
                }
            }
            "selected" => el.set_selected(v),
            "muted" => el.set_muted(v),
            _ => {
                let v = value.as_deref().unwrap_or("");
                if el.value() != v {
                    el.set_value(v);
                }
            }
        }
    } else {
        match value {
            Some(v) => el.set_attribute(name, v),
            None => el.remove_attribute(name),
        }
    }
}

fn set_ref(r: &NodeRef, el: &Node) {
    r.set(Some(el.clone().unchecked_into()));
}

/// Clears `r`, unless it now refers to another node.
fn clear_ref(r: &NodeRef, el: &Node) {
    if r.is(el.unchecked_ref()) {
        r.set(None);
    }
}

pub(crate) fn mount(html: &Html, pos: Pos<'_>, owner: Option<&Rc<CompState>>) -> M {
    match html {
        Html::Empty => M::Empty,
        Html::Text(s) => {
            let n = DOCUMENT.with(|d| d.create_text(s));
            insert(pos, &n);
            M::Text(n)
        }
        Html::Tpl(t) => M::Tpl(Box::new(mount_tpl(t, pos, owner))),
        Html::List(l) => {
            let parent = pos.parent();
            let pos = Pos::unstable(&parent, pos.before);
            M::List(l.children.iter().map(|c| mount(c, pos, owner)).collect())
        }
        Html::Comp(c) => M::Comp(CompState::mount(c, pos, owner)),
        Html::Raw(html) => M::Raw(mount_raw(html, pos)),
        Html::VRef(node) => {
            let node: &Node = node.unchecked_ref();
            insert(pos, node);
            vref_mounts(node, true);
            M::VRef(node.clone())
        }
    }
}

fn mount_raw(html: &str, pos: Pos<'_>) -> Vec<Node> {
    let parent = pos.parent();
    // Parse in the context of (a copy of) the element it's in, for e.g. SVG.
    let tmp = match context(parent.clone()) {
        Some(el) => el.clone_node(false),
        None => DOCUMENT.with(|d| d.create_element("div")),
    };
    tmp.set_inner_html(html);
    let mut nodes = Vec::new();
    while let Some(n) = tmp.first_child_opt() {
        parent.insert_before(&n, pos.before);
        nodes.push(n);
    }
    nodes
}

#[inline(never)]
fn mount_tpl(vt: &VTpl, pos: Pos<'_>, owner: Option<&Rc<CompState>>) -> MTpl {
    let walk = vt.tpl.walk;
    let bytes = walk.as_bytes();
    // (`<a>` is an SVG element in SVG.)
    let svg = bytes.first() == Some(&b'A') && context(pos.parent()).is_some_and(|el| is_svg(&el));
    let frag = instantiate(vt.tpl, svg);
    let mut outer = Outer(false);
    let mut cur = frag.clone();
    let mut stack: Vec<Node> = Vec::new();
    let mut roots = Vec::new();
    let mut holes = Vec::with_capacity(vt.dyns.len());
    let mut i = 0;
    while i < bytes.len() {
        let op = bytes[i];
        i += 1;
        let dyn_ = vt.dyns.get(holes.len());
        match op {
            b'f' => {
                let child = cur.first_child();
                stack.push(std::mem::replace(&mut cur, child));
            }
            b'n' => cur = cur.next_sibling().unwrap(),
            b'u' => cur = stack.pop().unwrap(),
            b'R' => roots.push(Root::Node(cur.clone())),
            b'W' | b'A' => {}
            b'a' | b'p' | b'e' | b'E' => {
                let start = i;
                while bytes[i] != b';' {
                    i += 1;
                }
                // SAFETY: names are ASCII, delimited by ASCII.
                let name = unsafe { walk.get_unchecked(start..i) };
                i += 1;
                match (op, dyn_) {
                    (b'a' | b'p', Some(Dyn::Attr(v))) => {
                        let prop = op == b'p';
                        if v.is_some() || prop {
                            set_attr(&cur, name, prop, v);
                        }
                        holes.push(MHole::Attr {
                            el: cur.clone(),
                            name,
                            prop,
                        });
                    }
                    (_, Some(Dyn::Listener(f))) => {
                        let mut l = Listener {
                            el: cur.clone(),
                            name,
                            passive: op == b'E',
                            f: Default::default(),
                            closure: None,
                        };
                        l.set(f);
                        holes.push(MHole::Listener(l));
                    }
                    _ => mismatch(),
                }
            }
            b'r' => {
                let Some(Dyn::Ref(r)) = dyn_ else { mismatch() };
                set_ref(r, &cur);
                holes.push(MHole::Ref(cur.clone(), r.clone()));
            }
            b'c' | b'C' | b'l' => {
                let Some(Dyn::Child(html)) = dyn_ else {
                    mismatch()
                };
                let (parent, anchor) = if op == b'l' {
                    (cur.clone(), None)
                } else {
                    (stack.last().unwrap().clone(), Some(cur.clone()))
                };
                // A top-level hole's parent changes (from the fragment), so
                // is found through its anchor.
                let hole_pos = if op == b'C' {
                    if !outer.0 {
                        // SAFETY: popped (by `outer`) before `pos` and
                        // `frag` are out of scope.
                        let entry = unsafe {
                            std::mem::transmute::<(Pos<'_>, &Node), (Pos<'static>, &'static Node)>(
                                (pos, &frag),
                            )
                        };
                        OUTER.with_borrow_mut(|o| o.push(entry));
                        outer.0 = true;
                    }
                    Pos::stable(None, anchor.as_ref())
                } else {
                    Pos::stable(Some(&parent), anchor.as_ref())
                };
                let m = mount(html, hole_pos, owner);
                if op == b'C' {
                    roots.push(Root::Hole(holes.len()));
                }
                holes.push(MHole::Child {
                    parent: (op != b'C').then_some(parent),
                    anchor,
                    m,
                });
            }
            _ => mismatch(),
        }
    }
    drop(outer);
    insert(pos, &frag);
    MTpl { roots, holes }
}

#[cold]
fn mismatch() -> ! {
    panic!("template mismatch")
}

pub(crate) fn patch(
    old: &Html,
    new: &Html,
    m: &mut M,
    pos: Pos<'_>,
    owner: Option<&Rc<CompState>>,
) {
    match (old, new, &mut *m) {
        (Html::Empty, Html::Empty, _) => return,
        (Html::Text(a), Html::Text(b), M::Text(n)) => {
            if a != b {
                n.set_data(b);
            }
            return;
        }
        (Html::Raw(a), Html::Raw(b), M::Raw(_)) if a == b => return,
        (Html::VRef(a), Html::VRef(b), M::VRef(_)) if a == b => return,
        (Html::Tpl(a), Html::Tpl(b), M::Tpl(t)) if std::ptr::eq(a.tpl, b.tpl) && a.key == b.key => {
            if !Rc::ptr_eq(a, b) {
                patch_tpl(a, b, t, owner);
            }
            return;
        }
        (Html::List(a), Html::List(b), M::List(ms)) if a.key == b.key => {
            if !Rc::ptr_eq(a, b) {
                patch_list(a, b, ms, pos, owner);
            }
            return;
        }
        (Html::Comp(a), Html::Comp(b), M::Comp(c)) if a.key == b.key && a.same_type(b) => {
            if !Rc::ptr_eq(a, b) {
                c.update(b);
            }
            return;
        }
        _ => {}
    }
    // Replace. (`pos` is just after the old content.)
    let new_m = mount(new, pos, owner);
    std::mem::replace(m, new_m).unmount(true);
}

#[inline(never)]
fn patch_tpl(old: &VTpl, new: &VTpl, t: &mut MTpl, owner: Option<&Rc<CompState>>) {
    for (h, (o, n)) in t.holes.iter_mut().zip(old.dyns.iter().zip(&new.dyns)) {
        match (h, o, n) {
            (MHole::Attr { el, name, prop }, Dyn::Attr(o), Dyn::Attr(n)) => {
                if o != n || *prop {
                    set_attr(el, name, *prop, n);
                }
            }
            (MHole::Listener(l), _, Dyn::Listener(n)) => l.set(n),
            (MHole::Ref(el, r), Dyn::Ref(_), Dyn::Ref(n)) => {
                if r != n {
                    clear_ref(r, el);
                    set_ref(n, el);
                    *r = n.clone();
                }
            }
            (MHole::Child { parent, anchor, m }, Dyn::Child(o), Dyn::Child(n)) => {
                let pos = Pos::stable(parent.as_ref(), anchor.as_ref());
                patch(o, n, m, pos, owner);
            }
            _ => mismatch(),
        }
    }
}

#[inline(never)]
fn patch_list(
    old: &VList,
    new: &VList,
    ms: &mut Vec<M>,
    pos: Pos<'_>,
    owner: Option<&Rc<CompState>>,
) {
    let parent = pos.parent();
    let keyed = matches!((old.children.first(), new.children.first()), (Some(a), Some(b)) if a.key().is_some() && b.key().is_some());
    if !keyed {
        let common = old.children.len().min(new.children.len());
        // Remove extra old items.
        for m in ms.drain(common..) {
            m.unmount(true);
        }
        // Mount extra new items, before whatever follows the list.
        let mut next = pos.before.cloned();
        if new.children.len() > common {
            let pos = Pos::unstable(&parent, next.as_ref());
            let start = ms.len();
            for c in &new.children[common..] {
                ms.push(mount(c, pos, owner));
            }
            next = ms[start..].iter().find_map(M::first_node).or(next);
        }
        for i in (0..common).rev() {
            let pos = Pos::unstable(&parent, next.as_ref());
            patch(&old.children[i], &new.children[i], &mut ms[i], pos, owner);
            if let Some(f) = ms[i].first_node() {
                next = Some(f);
            }
        }
        return;
    }

    // Keyed: match new items to old by key.
    let mut olds: Vec<Option<M>> = std::mem::take(ms).into_iter().map(Some).collect();
    let mut taken = vec![false; old.children.len()];
    let mut sources: Vec<Option<usize>> = Vec::with_capacity(new.children.len());
    for (j, c) in new.children.iter().enumerate() {
        let key = c.key();
        // Fast path: same position.
        let found = if j < taken.len() && !taken[j] && old.children[j].key() == key {
            Some(j)
        } else {
            (0..taken.len()).find(|&i| !taken[i] && old.children[i].key() == key)
        };
        if let Some(i) = found {
            taken[i] = true;
        }
        sources.push(found);
    }
    // Remove unmatched old items first, so they don't cause moves.
    for (o, taken) in olds.iter_mut().zip(taken) {
        if !taken {
            o.take().unwrap().unmount(true);
        }
    }
    // Items that keep their relative order (a longest increasing
    // subsequence of their old positions) stay put; the rest move.
    let stay = lis(&sources);
    let mut next = pos.before.cloned();
    let mut result: Vec<M> = Vec::with_capacity(new.children.len());
    for j in (0..new.children.len()).rev() {
        let pos = Pos::unstable(&parent, next.as_ref());
        let m = match sources[j] {
            Some(i) => {
                let mut m = olds[i].take().unwrap();
                patch(&old.children[i], &new.children[j], &mut m, pos, owner);
                if !stay[j] {
                    m.move_before(pos);
                }
                m
            }
            None => mount(&new.children[j], pos, owner),
        };
        if let Some(f) = m.first_node() {
            next = Some(f);
        }
        result.push(m);
    }
    result.reverse();
    *ms = result;
}

/// Which items are in a longest increasing subsequence of `sources` (an
/// old index per new item, if any).
fn lis(sources: &[Option<usize>]) -> Vec<bool> {
    // `tails[k]`: the item ending the best subsequence of length `k + 1`.
    let mut tails: Vec<usize> = Vec::new();
    let mut prev = vec![usize::MAX; sources.len()];
    for (j, s) in sources.iter().enumerate() {
        let Some(s) = *s else { continue };
        let k = tails.partition_point(|&t| sources[t].unwrap() < s);
        if k > 0 {
            prev[j] = tails[k - 1];
        }
        if k == tails.len() {
            tails.push(j);
        } else {
            tails[k] = j;
        }
    }
    let mut stay = vec![false; sources.len()];
    let mut j = tails.last().copied().unwrap_or(usize::MAX);
    while j != usize::MAX {
        stay[j] = true;
        j = prev[j];
    }
    stay
}
