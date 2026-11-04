//! Component instances, the render scheduler and hook storage.
//!
//! Everything here is non-generic except the thin [`PropsOf`] vtable, so a
//! component costs little more than its own render function.

use crate::dom::Node;
use crate::dom::{self, M, Pos};
use crate::vdom::{Html, Key};
use std::any::{Any, TypeId};
use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::rc::{Rc, Weak};

/// Properties of a component.
pub trait Properties: PartialEq {
    #[doc(hidden)]
    type Builder;
    #[doc(hidden)]
    fn builder() -> Self::Builder;
}

#[doc(hidden)]
pub struct UnitBuilder;
impl UnitBuilder {
    #[inline(always)]
    pub fn build(self) {}
}

/// A properties builder with every required property set.
#[doc(hidden)]
pub trait Build {
    type Output;
    fn __build(self) -> Self::Output;
}

impl Build for UnitBuilder {
    type Output = ();
    #[inline(always)]
    fn __build(self) {}
}

/// Builds the properties and the component together (one call per use).
#[doc(hidden)]
#[inline(never)]
pub fn __comp_b<C: BaseComponent, B: Build<Output = C::Properties>>(
    key: Option<Key>,
    b: B,
) -> Html {
    __comp::<C>(key, b.__build())
}

impl Properties for () {
    type Builder = UnitBuilder;
    #[inline(always)]
    fn builder() -> UnitBuilder {
        UnitBuilder
    }
}

/// Something that can be used as `<Component/>` in `html!`.
pub trait BaseComponent: Sized + 'static {
    /// The component's properties.
    type Properties: Properties + 'static;
    #[doc(hidden)]
    fn __run(props: &Self::Properties, state: &Rc<CompState>) -> Option<Html>;
}

/// Type-erased properties of a particular component type.
pub trait AnyComp {
    /// Whether the properties equal `other`'s, which must be of the same
    /// component type.
    fn props_eq(&self, other: &dyn AnyComp) -> bool;
    fn run(&self, state: &Rc<CompState>) -> Option<Html>;
}

#[doc(hidden)]
pub struct PropsOf<C: BaseComponent> {
    pub(crate) props: C::Properties,
    pub(crate) _c: PhantomData<fn() -> C>,
}

impl<C: BaseComponent> AnyComp for PropsOf<C> {
    fn props_eq(&self, other: &dyn AnyComp) -> bool {
        // SAFETY: the caller checked the type.
        let other = unsafe { &*(other as *const dyn AnyComp as *const Self) };
        other.props == self.props
    }
    fn run(&self, state: &Rc<CompState>) -> Option<Html> {
        C::__run(&self.props, state)
    }
}

/// A component in [`Html`], with its key and properties.
pub struct VCompG<T: ?Sized + AnyComp> {
    pub(crate) key: Option<Key>,
    /// Of the `PropsOf<C>`, identifying the component type.
    pub(crate) type_id: TypeId,
    pub(crate) props: T,
}
pub type VComp = VCompG<dyn AnyComp>;

impl VComp {
    pub(crate) fn same_type(&self, other: &VComp) -> bool {
        self.type_id == other.type_id
    }
}

#[doc(hidden)]
#[inline]
pub fn __comp<C: BaseComponent>(key: Option<Key>, props: C::Properties) -> Html {
    let rc: Rc<VComp> = Rc::new(VCompG {
        key,
        type_id: TypeId::of::<PropsOf<C>>(),
        props: PropsOf::<C> {
            props,
            _c: PhantomData,
        },
    });
    Html::Comp(rc)
}

/// A mounted component.
pub struct CompState {
    pub(crate) vcomp: RefCell<Rc<VComp>>,
    pub(crate) hooks: RefCell<Vec<Rc<dyn Any>>>,
    pub(crate) destroyers: RefCell<Vec<Rc<dyn Destroy>>>,
    pub(crate) m: RefCell<M>,
    pub(crate) last: RefCell<Html>,
    /// Where the component's content goes: before `before` in `parent_node`
    /// (if `None`, `before`'s parent). Stable for the component's life:
    /// either its position in a template, or (for an item of a list, which
    /// can move) an empty text node of its own after its content.
    pub(crate) parent_node: Option<Node>,
    pub(crate) before: Option<Node>,
    pub(crate) own_anchor: bool,
    pub(crate) parent: Option<Weak<CompState>>,
    /// A context this component provides (a `ProviderCell<T>`).
    pub(crate) provides: RefCell<Option<Rc<dyn Any>>>,
    pub(crate) depth: u32,
    pub(crate) dirty: Cell<bool>,
    pub(crate) dead: Cell<bool>,
}

/// Something to do when a component is destroyed.
pub(crate) trait Destroy {
    fn destroy(&self);
}

thread_local! {
    static CURRENT: RefCell<Option<Rc<CompState>>> = const { RefCell::new(None) };
    static HOOK_INDEX: Cell<usize> = const { Cell::new(0) };
    static QUEUE: RefCell<Vec<Rc<CompState>>> = const { RefCell::new(Vec::new()) };
    static EFFECTS: RefCell<Vec<Box<dyn FnOnce()>>> = const { RefCell::new(Vec::new()) };
    static SCHEDULED: Cell<bool> = const { Cell::new(false) };
    static FLUSHING: Cell<bool> = const { Cell::new(false) };
    static ROOTS: RefCell<Vec<Rc<CompState>>> = const { RefCell::new(Vec::new()) };
}

impl CompState {
    #[inline(never)]
    pub(crate) fn mount(
        vcomp: &Rc<VComp>,
        pos: Pos<'_>,
        owner: Option<&Rc<CompState>>,
    ) -> Rc<CompState> {
        let own_anchor = !pos.stable;
        let (parent_node, before) = if own_anchor {
            let anchor = dom::empty_text();
            dom::insert(pos, &anchor);
            (None, Some(anchor))
        } else {
            (pos.parent.cloned(), pos.before.cloned())
        };
        let state = Rc::new(CompState {
            vcomp: RefCell::new(vcomp.clone()),
            hooks: Default::default(),
            destroyers: Default::default(),
            m: RefCell::new(M::Empty),
            last: Default::default(),
            parent_node,
            before,
            own_anchor,
            parent: owner.map(Rc::downgrade),
            provides: Default::default(),
            depth: owner.map_or(0, |o| o.depth + 1),
            dirty: Cell::new(false),
            dead: Cell::new(false),
        });
        state.render();
        state
    }

    /// Updates the properties, rendering if they changed.
    pub(crate) fn update(self: &Rc<Self>, vcomp: &Rc<VComp>) {
        let changed = {
            let mut cur = self.vcomp.borrow_mut();
            if Rc::ptr_eq(&cur, vcomp)
                || (cur.type_id == vcomp.type_id && cur.props.props_eq(&vcomp.props))
            {
                false
            } else {
                *cur = vcomp.clone();
                true
            }
        };
        if changed || self.dirty.get() {
            self.render();
        }
    }

    /// Runs the component (with its hooks), returning what it renders, or
    /// `None` if it need not re-render.
    pub(crate) fn run(self: &Rc<Self>) -> Option<Html> {
        /// Restores the current component, also if it panics (a server may
        /// catch that, and render again).
        struct Restore(Option<Rc<CompState>>, usize);
        impl Drop for Restore {
            fn drop(&mut self) {
                HOOK_INDEX.set(self.1);
                CURRENT.with(|c| *c.borrow_mut() = self.0.take());
            }
        }
        let _restore = Restore(
            CURRENT.with(|c| c.replace(Some(self.clone()))),
            HOOK_INDEX.replace(0),
        );
        let vcomp = self.vcomp.borrow().clone();
        vcomp.props.run(self)
    }

    /// A component for server rendering, which has no DOM.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn new_ssr(vcomp: &Rc<VComp>, owner: Option<&Rc<CompState>>) -> Rc<CompState> {
        Rc::new(CompState {
            vcomp: RefCell::new(vcomp.clone()),
            hooks: Default::default(),
            destroyers: Default::default(),
            m: RefCell::new(M::Empty),
            last: Default::default(),
            parent_node: None,
            before: None,
            own_anchor: false,
            parent: owner.map(Rc::downgrade),
            provides: Default::default(),
            depth: owner.map_or(0, |o| o.depth + 1),
            dirty: Cell::new(false),
            dead: Cell::new(false),
        })
    }

    pub(crate) fn render(self: &Rc<Self>) {
        self.dirty.set(false);
        if self.dead.get() {
            return;
        }
        let start = EFFECTS.with_borrow(Vec::len);
        let html = self.run();
        let Some(html) = html else {
            return;
        };
        // As yew: this component's effects run after those of the children
        // it mounts or updates now.
        let effects = EFFECTS.with_borrow_mut(|e| e.split_off(start));
        let old = self.last.replace(Html::Empty);
        let mut m = self.m.take();
        let pos = self.pos();
        if m.is_empty() && old.is_empty() {
            m = dom::mount(&html, pos, Some(self));
        } else {
            dom::patch(&old, &html, &mut m, pos, Some(self));
        }
        *self.m.borrow_mut() = m;
        *self.last.borrow_mut() = html;
        EFFECTS.with_borrow_mut(|e| e.extend(effects));
    }

    pub(crate) fn unmount(&self, remove: bool) {
        self.dead.set(true);
        for d in self.destroyers.take() {
            d.destroy();
        }
        self.m.take().unmount(remove);
        if remove && self.own_anchor {
            dom::remove(self.before.as_ref().unwrap());
        }
        self.hooks.take();
        self.provides.take();
        self.last.take();
    }

    fn pos(&self) -> Pos<'_> {
        Pos::stable(self.parent_node.as_ref(), self.before.as_ref())
    }

    pub(crate) fn first_node(&self) -> Option<Node> {
        let first = self.m.borrow().first_node();
        first.or_else(|| self.own_anchor.then(|| self.before.clone().unwrap()))
    }

    pub(crate) fn move_before(&self, pos: Pos<'_>) {
        self.m.borrow().move_before(pos);
        if self.own_anchor {
            dom::insert(pos, self.before.as_ref().unwrap());
        }
    }

    pub(crate) fn parent(&self) -> Option<Rc<CompState>> {
        self.parent.as_ref().and_then(Weak::upgrade)
    }
}

/// The component currently rendering.
#[doc(hidden)]
pub fn current() -> Rc<CompState> {
    CURRENT
        .with(|c| c.borrow().clone())
        .expect("hooks must be called while rendering a component")
}

/// The `index`th hook slot of the current component, created by `init` the
/// first time. Non-generic.
#[inline(never)]
fn hook_slot(type_id: TypeId, init: Thunk<'_, (), Rc<dyn Any>>) -> Rc<dyn Any> {
    let comp = current();
    let index = HOOK_INDEX.get();
    HOOK_INDEX.set(index + 1);
    let existing = comp.hooks.borrow().get(index).cloned();
    let slot = match existing {
        Some(slot) => slot,
        None => {
            let slot = init.call(());
            comp.hooks.borrow_mut().push(slot.clone());
            slot
        }
    };
    assert!(
        (*slot).type_id() == type_id,
        "hooks called in a different order"
    );
    slot
}

/// The current component's next hook slot, holding a `T` made by `init` the
/// first time.
#[doc(hidden)]
#[inline]
pub fn use_slot<T: 'static>(init: Thunk<'_, (), T>) -> Rc<T> {
    let slot = hook_slot(
        TypeId::of::<T>(),
        Thunk::new(&mut Some(move |()| Rc::new(init.call(())) as Rc<dyn Any>)),
    );
    // SAFETY: checked by `hook_slot`.
    unsafe { Rc::from_raw(Rc::into_raw(slot) as *const T) }
}

/// A `FnOnce(A) -> T`, erased without a vtable: a pointer to an
/// `Option<F>` (owned by the caller) and the one function that calls it.
#[doc(hidden)]
pub struct Thunk<'a, A, T> {
    data: *mut (),
    call: unsafe fn(*mut (), A) -> T,
    _marker: PhantomData<&'a mut ()>,
}

unsafe fn call_thunk<A, T, F: FnOnce(A) -> T>(p: *mut (), a: A) -> T {
    // SAFETY: `Thunk::new` stored a `Some(F)` here, and calls it once.
    unsafe { (*(p as *mut Option<F>)).take().unwrap_unchecked()(a) }
}

impl<'a, A, T> Thunk<'a, A, T> {
    /// `f` must be `Some`.
    #[inline(always)]
    pub fn new<F: FnOnce(A) -> T>(f: &'a mut Option<F>) -> Self {
        Self {
            data: f as *mut Option<F> as *mut (),
            call: call_thunk::<A, T, F>,
            _marker: PhantomData,
        }
    }

    #[inline(always)]
    pub fn call(self, a: A) -> T {
        // SAFETY: called at most once (this consumes `self`).
        unsafe { (self.call)(self.data, a) }
    }
}

/// Registers something to do when the current component is destroyed.
pub(crate) fn on_destroy(d: Rc<dyn Destroy>) {
    current().destroyers.borrow_mut().push(d);
}

/// Schedules a component to render.
#[inline(never)]
pub(crate) fn schedule(comp: &Weak<CompState>) {
    let Some(comp) = comp.upgrade() else {
        return;
    };
    if comp.dirty.replace(true) || comp.dead.get() {
        return;
    }
    QUEUE.with_borrow_mut(|q| q.push(comp));
    request_flush();
}

/// Runs `f` after the current render (or soon, if not rendering).
pub(crate) fn push_effect(f: Box<dyn FnOnce()>) {
    if ssr() {
        // Effects don't run on the server.
        return;
    }
    EFFECTS.with_borrow_mut(|e| e.push(f));
    request_flush();
}

fn request_flush() {
    if FLUSHING.get() || ssr() || SCHEDULED.replace(true) {
        return;
    }
    dom::queue_microtask();
}

thread_local! {
    static SSR: Cell<bool> = const { Cell::new(false) };
}

/// Whether rendering on the server.
#[inline]
pub(crate) fn ssr() -> bool {
    #[cfg(target_arch = "wasm32")]
    return false;
    #[cfg(not(target_arch = "wasm32"))]
    SSR.get()
}

/// Runs `f` in server rendering mode.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn with_ssr<R>(f: impl FnOnce() -> R) -> R {
    /// Also if rendering panics.
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            SSR.set(self.0);
            // (What rendering scheduled: nothing renders again.)
            QUEUE.take();
        }
    }
    let _restore = Restore(SSR.replace(true));
    f()
}

/// Renders everything scheduled, then runs effects, until quiescent.
pub(crate) fn flush() {
    SCHEDULED.set(false);
    if FLUSHING.replace(true) {
        return;
    }
    loop {
        let mut queue = QUEUE.take();
        if queue.is_empty() {
            let effects = EFFECTS.take();
            if effects.is_empty() {
                break;
            }
            for e in effects {
                e();
            }
            continue;
        }
        // Parents before children, so a child re-rendered by its parent
        // renders once. (Insertion sort: small, and the queue is short.)
        for i in 1..queue.len() {
            let mut j = i;
            while j > 0 && queue[j - 1].depth > queue[j].depth {
                queue.swap(j - 1, j);
                j -= 1;
            }
        }
        for c in queue {
            if c.dirty.get() {
                c.render();
            }
        }
    }
    FLUSHING.set(false);
}

/// Mounts `C` (with properties) as the last child of `root`.
pub(crate) fn mount_root(html: Html, root: &Node) -> Rc<CompState> {
    let Html::Comp(vcomp) = html else {
        unreachable!()
    };
    // As yew: the root's previous contents are replaced.
    dom::clear(root);
    // (Possibly while already flushing, e.g. from an effect.)
    let flushing = FLUSHING.replace(true);
    let state = CompState::mount(&vcomp, Pos::append(root), None);
    FLUSHING.set(flushing);
    if !flushing {
        flush();
    }
    ROOTS.with_borrow_mut(|r| r.push(state.clone()));
    state
}

/// Unmounts a root component.
pub(crate) fn unmount_root(state: &Rc<CompState>) {
    ROOTS.with_borrow_mut(|r| r.retain(|s| !Rc::ptr_eq(s, state)));
    state.unmount(true);
}
