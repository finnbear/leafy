//! Hooks, as yew's.
//!
//! Each public hook is a thin generic wrapper that erases its closure
//! argument and calls an out-of-line function generic only over the state
//! type, so a call site costs little more than a call.

use crate::component::{
    CompState, Destroy, Thunk, current, on_destroy, push_effect, schedule, use_slot,
};
use crate::{Callback, NodeRef};
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::fmt;
use std::ops::Deref;
use std::rc::{Rc, Weak};

fn current_weak() -> Weak<CompState> {
    Rc::downgrade(&current())
}

/// Erases a `FnOnce() -> T` into a [`Thunk`].
macro_rules! once {
    ($f:expr) => {
        Thunk::new(&mut Some(move |()| ($f)()))
    };
}

// ---------------------------------------------------------------- use_state

struct StateCell<T> {
    value: RefCell<Rc<T>>,
    comp: Weak<CompState>,
    eq: Option<fn(&T, &T) -> bool>,
}

impl<T> StateCell<T> {
    #[inline(never)]
    fn set(&self, value: T) {
        if let Some(eq) = self.eq
            && eq(&self.value.borrow(), &value)
        {
            return;
        }
        *self.value.borrow_mut() = Rc::new(value);
        schedule(&self.comp);
    }
}

/// The handle returned by [`use_state`]: dereferences to the value as of the
/// render it was returned by.
pub struct UseStateHandle<T> {
    inner: std::mem::ManuallyDrop<StateHandleInner<T>>,
}

struct StateHandleInner<T> {
    value: Rc<T>,
    cell: Rc<StateCell<T>>,
}

// Out of line, as handles are cloned into and dropped with many closures.
impl<T> Clone for UseStateHandle<T> {
    #[inline(never)]
    fn clone(&self) -> Self {
        Self {
            inner: std::mem::ManuallyDrop::new(StateHandleInner {
                value: self.inner.value.clone(),
                cell: self.inner.cell.clone(),
            }),
        }
    }
}

impl<T> Drop for UseStateHandle<T> {
    #[inline(never)]
    fn drop(&mut self) {
        // SAFETY: dropped once, here.
        unsafe { std::mem::ManuallyDrop::drop(&mut self.inner) }
    }
}

impl<T> Deref for UseStateHandle<T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &T {
        &self.inner.value
    }
}

impl<T: PartialEq> PartialEq for UseStateHandle<T> {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

impl<T: fmt::Debug> fmt::Debug for UseStateHandle<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (**self).fmt(f)
    }
}

impl<T: fmt::Display> fmt::Display for UseStateHandle<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (**self).fmt(f)
    }
}

impl<T: 'static> UseStateHandle<T> {
    /// Replaces the value and re-renders.
    #[inline]
    pub fn set(&self, value: T) {
        self.inner.cell.set(value);
    }

    /// A [`UseStateSetter`] for this state.
    pub fn setter(&self) -> UseStateSetter<T> {
        UseStateSetter {
            cell: self.inner.cell.clone(),
        }
    }
}

/// Sets the value of a [`use_state`] without being able to read it.
pub struct UseStateSetter<T> {
    cell: Rc<StateCell<T>>,
}

impl<T> Clone for UseStateSetter<T> {
    #[inline(never)]
    fn clone(&self) -> Self {
        Self {
            cell: self.cell.clone(),
        }
    }
}

impl<T> PartialEq for UseStateSetter<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.cell, &other.cell)
    }
}

impl<T: 'static> UseStateSetter<T> {
    /// Replaces the value and re-renders.
    #[inline]
    pub fn set(&self, value: T) {
        self.cell.set(value);
    }
}

#[inline(never)]
fn use_state_impl<T: 'static>(
    init: Thunk<'_, (), T>,
    eq: Option<fn(&T, &T) -> bool>,
) -> UseStateHandle<T> {
    let cell = use_slot(Thunk::new(&mut Some(move |()| StateCell {
        value: RefCell::new(Rc::new(init.call(()))),
        comp: current_weak(),
        eq,
    })));
    let value = cell.value.borrow().clone();
    UseStateHandle {
        inner: std::mem::ManuallyDrop::new(StateHandleInner { value, cell }),
    }
}

/// State that re-renders the component when set.
#[inline]
pub fn use_state<T: 'static, F: FnOnce() -> T>(init: F) -> UseStateHandle<T> {
    use_state_impl(once!(init), None)
}

/// As [`use_state`], but setting an equal value does nothing.
#[inline]
pub fn use_state_eq<T: PartialEq + 'static, F: FnOnce() -> T>(init: F) -> UseStateHandle<T> {
    use_state_impl(once!(init), Some(T::eq))
}

// -------------------------------------------------------------- use_reducer

/// State changed by actions.
pub trait Reducible {
    /// What changes the state.
    type Action;
    /// The state after `action`. Returning `self` (the same `Rc`) does not
    /// re-render.
    fn reduce(self: Rc<Self>, action: Self::Action) -> Rc<Self>;
}

struct ReducerCell<R> {
    value: RefCell<Rc<R>>,
    comp: Weak<CompState>,
    eq: Option<fn(&R, &R) -> bool>,
}

impl<R: Reducible + 'static> ReducerCell<R> {
    #[inline(never)]
    fn dispatch(&self, action: R::Action) {
        let cur = self.value.borrow().clone();
        let next = cur.clone().reduce(action);
        let changed = !Rc::ptr_eq(&cur, &next) && !self.eq.is_some_and(|eq| eq(&cur, &next));
        if changed {
            *self.value.borrow_mut() = next;
            schedule(&self.comp);
        }
    }
}

/// The handle returned by [`use_reducer`]: dereferences to the state as of
/// the render it was returned by.
pub struct UseReducerHandle<R> {
    value: Rc<R>,
    cell: Rc<ReducerCell<R>>,
}

impl<R> Clone for UseReducerHandle<R> {
    #[inline(never)]
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            cell: self.cell.clone(),
        }
    }
}

impl<R> Deref for UseReducerHandle<R> {
    type Target = R;
    fn deref(&self) -> &R {
        &self.value
    }
}

impl<R: PartialEq> PartialEq for UseReducerHandle<R> {
    fn eq(&self, other: &Self) -> bool {
        *self.value == *other.value
    }
}

impl<R: Reducible + 'static> UseReducerHandle<R> {
    /// Applies `action` to the state, re-rendering if it changed.
    pub fn dispatch(&self, action: R::Action) {
        self.cell.dispatch(action);
    }

    /// A [`UseReducerDispatcher`] for this state.
    pub fn dispatcher(&self) -> UseReducerDispatcher<R> {
        UseReducerDispatcher {
            cell: self.cell.clone(),
        }
    }
}

/// Dispatches actions to a [`use_reducer`] without being able to read its
/// state.
pub struct UseReducerDispatcher<R> {
    cell: Rc<ReducerCell<R>>,
}

impl<R> Clone for UseReducerDispatcher<R> {
    #[inline(never)]
    fn clone(&self) -> Self {
        Self {
            cell: self.cell.clone(),
        }
    }
}

impl<R> PartialEq for UseReducerDispatcher<R> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.cell, &other.cell)
    }
}

impl<R: Reducible + 'static> UseReducerDispatcher<R> {
    /// Applies `action` to the state, re-rendering if it changed.
    pub fn dispatch(&self, action: R::Action) {
        self.cell.dispatch(action);
    }

    /// A callback that dispatches its input.
    pub fn to_callback(&self) -> Callback<R::Action> {
        let cell = self.cell.clone();
        Callback::from(move |a| cell.dispatch(a))
    }
}

#[inline(never)]
fn use_reducer_impl<R: Reducible + 'static>(
    init: Thunk<'_, (), R>,
    eq: Option<fn(&R, &R) -> bool>,
) -> UseReducerHandle<R> {
    let cell = use_slot(Thunk::new(&mut Some(move |()| ReducerCell {
        value: RefCell::new(Rc::new(init.call(()))),
        comp: current_weak(),
        eq,
    })));
    let value = cell.value.borrow().clone();
    UseReducerHandle { value, cell }
}

/// State changed by dispatching actions to a [`Reducible`]. Re-renders
/// unless [`Reducible::reduce`] returns the same `Rc`.
#[inline]
pub fn use_reducer<R: Reducible + 'static, F: FnOnce() -> R>(init: F) -> UseReducerHandle<R> {
    use_reducer_impl(once!(init), None)
}

/// As [`use_reducer`], but an action that results in an equal state does
/// nothing.
#[inline]
pub fn use_reducer_eq<R: Reducible + PartialEq + 'static, F: FnOnce() -> R>(
    init: F,
) -> UseReducerHandle<R> {
    use_reducer_impl(once!(init), Some(R::eq))
}

// ------------------------------------------------------------- simple hooks

#[inline(never)]
fn use_mut_ref_impl<T: 'static>(init: Thunk<'_, (), T>) -> Rc<RefCell<T>> {
    use_slot(Thunk::new(&mut Some(move |()| RefCell::new(init.call(())))))
}

/// A mutable value that persists across renders without causing them.
#[inline]
pub fn use_mut_ref<T: 'static, F: FnOnce() -> T>(init: F) -> Rc<RefCell<T>> {
    use_mut_ref_impl(once!(init))
}

#[inline(never)]
fn use_ref_impl<T: 'static>(init: Thunk<'_, (), T>) -> Rc<T> {
    use_slot(init)
}

/// An immutable value that persists across renders.
#[inline]
pub fn use_ref<T: 'static, F: FnOnce() -> T>(init: F) -> Rc<T> {
    use_ref_impl(once!(init))
}

/// A [`NodeRef`] that persists across renders.
#[inline(never)]
pub fn use_node_ref() -> NodeRef {
    (*use_slot(Thunk::new(&mut Some(|()| NodeRef::default())))).clone()
}

/// The handle returned by [`use_force_update`].
#[derive(Clone)]
pub struct UseForceUpdateHandle {
    comp: Weak<CompState>,
}

impl PartialEq for UseForceUpdateHandle {
    fn eq(&self, other: &Self) -> bool {
        self.comp.ptr_eq(&other.comp)
    }
}

impl UseForceUpdateHandle {
    /// Re-renders the component.
    pub fn force_update(&self) {
        schedule(&self.comp);
    }
}

/// A handle that re-renders the component on demand.
#[inline(never)]
pub fn use_force_update() -> UseForceUpdateHandle {
    let comp = use_slot(Thunk::new(&mut Some(|()| current_weak())));
    UseForceUpdateHandle {
        comp: (*comp).clone(),
    }
}

type MemoSlot<T, D> = Rc<RefCell<Option<(D, Rc<T>)>>>;

/// The memo's slot, and its value if `deps` are unchanged.
#[inline(never)]
fn use_memo_impl<T: 'static, D: PartialEq + 'static>(deps: &D) -> (MemoSlot<T, D>, Option<Rc<T>>) {
    let slot = use_slot(Thunk::new(&mut Some(|()| RefCell::new(None::<(D, Rc<T>)>))));
    let v = match &*slot.borrow() {
        Some((d, v)) if d == deps => Some(v.clone()),
        _ => None,
    };
    (slot, v)
}

/// Recomputes `f(&deps)` only when `deps` change.
#[inline]
pub fn use_memo<T: 'static, D: PartialEq + 'static, F: FnOnce(&D) -> T>(deps: D, f: F) -> Rc<T> {
    let (slot, v) = use_memo_impl::<T, D>(&deps);
    if let Some(v) = v {
        return v;
    }
    let v = Rc::new(f(&deps));
    *slot.borrow_mut() = Some((deps, v.clone()));
    v
}

/// A callback recreated only when `deps` change.
pub fn use_callback<IN: 'static, OUT: 'static, F, D>(deps: D, f: F) -> Callback<IN, OUT>
where
    F: Fn(IN, &D) -> OUT + 'static,
    D: PartialEq + 'static,
{
    let deps = Rc::new(deps);
    (*use_memo(deps, move |deps| {
        let deps = deps.clone();
        Callback::from(move |input| f(input, &deps))
    }))
    .clone()
}

// ---------------------------------------------------------------- effects

/// What an effect returns: something to run before the next effect or on
/// destruction.
pub trait TearDown: 'static {
    /// Runs the teardown.
    fn tear_down(self);
}

impl TearDown for () {
    fn tear_down(self) {}
}

impl<F: FnOnce() + 'static> TearDown for F {
    fn tear_down(self) {
        self()
    }
}

/// A boxed [`TearDown`], without a vtable.
struct TearBox {
    data: *mut (),
    run: unsafe fn(*mut ()),
}

unsafe fn run_teardown<T: TearDown>(p: *mut ()) {
    // SAFETY: made by `TearBox::new::<T>`.
    let t = unsafe { *Box::from_raw(p as *mut T) };
    t.tear_down();
}

impl TearBox {
    fn new<T: TearDown>(t: T) -> Self {
        Self {
            data: Box::into_raw(Box::new(t)) as *mut (),
            run: run_teardown::<T>,
        }
    }

    fn run(self) {
        // SAFETY: run once.
        unsafe { (self.run)(self.data) }
    }
}

/// A `use_effect*` hook.
#[derive(Default)]
struct EffectCell {
    /// The dependencies it last ran with.
    deps: RefCell<Option<Rc<dyn Any>>>,
    destructor: RefCell<Option<TearBox>>,
    /// As in yew: the latest render's effect, to run (once) after rendering.
    pending: RefCell<Option<Box<dyn Effect>>>,
    registered: Cell<bool>,
    dead: Cell<bool>,
}

impl EffectCell {
    fn run_pending(&self) {
        let Some(mut effect) = self.pending.take() else {
            return;
        };
        if self.dead.get() {
            return;
        }
        let changed = match &*self.deps.borrow() {
            Some(old) => effect.changed(&**old),
            None => true,
        };
        if changed {
            if let Some(d) = self.destructor.take() {
                d.run();
            }
            let deps = effect.deps();
            *self.deps.borrow_mut() = Some(deps.clone());
            *self.destructor.borrow_mut() = effect.run(&*deps);
        }
    }
}

impl Destroy for EffectCell {
    fn destroy(&self) {
        self.dead.set(true);
        self.pending.take();
        if let Some(d) = self.destructor.take() {
            d.run();
        }
    }
}

/// One `use_effect*` call: its dependencies and function.
trait Effect {
    /// Whether to run, given the previous dependencies.
    fn changed(&self, old: &dyn Any) -> bool;
    /// Moves the dependencies out, to keep.
    fn deps(&mut self) -> Rc<dyn Any>;
    /// Runs the effect, returning its teardown.
    fn run(self: Box<Self>, deps: &dyn Any) -> Option<TearBox>;
}

struct EffectImpl<D, F> {
    deps: Option<D>,
    f: F,
    /// `None` to run on every render.
    eq: Option<fn(&D, &D) -> bool>,
}

impl<D: 'static, T: TearDown, F: FnOnce(&D) -> T> Effect for EffectImpl<D, F> {
    fn changed(&self, old: &dyn Any) -> bool {
        match (self.eq, old.downcast_ref::<D>(), &self.deps) {
            (Some(eq), Some(old), Some(new)) => !eq(old, new),
            _ => true,
        }
    }

    fn deps(&mut self) -> Rc<dyn Any> {
        Rc::new(self.deps.take().unwrap())
    }

    fn run(self: Box<Self>, deps: &dyn Any) -> Option<TearBox> {
        // SAFETY: `deps` is what `Self::deps` returned.
        let deps = unsafe { &*(deps as *const dyn Any as *const D) };
        let t = (self.f)(deps);
        // Avoid allocating for `()`.
        if std::any::TypeId::of::<T>() == std::any::TypeId::of::<()>() {
            None
        } else {
            Some(TearBox::new(t))
        }
    }
}

/// The non-generic part of effects.
#[inline(never)]
fn use_effect_base(effect: Box<dyn Effect>) {
    let cell = use_slot(Thunk::new(&mut Some(|()| EffectCell::default())));
    if !cell.registered.replace(true) {
        on_destroy(cell.clone());
    }
    // Run after rendering, unless already to run (with an earlier render's
    // effect, which this replaces).
    if cell.pending.replace(Some(effect)).is_none() {
        push_effect(Box::new(move || cell.run_pending()));
    }
}

/// Runs `f` after rendering, whenever `deps` change.
#[inline]
pub fn use_effect_with<D: PartialEq + 'static, T: TearDown, F: FnOnce(&D) -> T + 'static>(
    deps: D,
    f: F,
) {
    use_effect_base(Box::new(EffectImpl {
        deps: Some(deps),
        f,
        eq: Some(D::eq),
    }));
}

/// Runs `f` after every render.
#[inline]
pub fn use_effect<T: TearDown, F: FnOnce() -> T + 'static>(f: F) {
    use_effect_base(Box::new(EffectImpl {
        deps: Some(()),
        f: move |_: &()| f(),
        eq: None,
    }));
}

// -------------------------------------------------------------- use_pending

/// The handle returned by [`use_pending`]: dereferences to the value to
/// show (as of the render it was returned by).
pub struct UsePendingHandle<T> {
    value: T,
    pending: bool,
    pick: UseStateSetter<Option<T>>,
}

impl<T: Clone> Clone for UsePendingHandle<T> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            pending: self.pending,
            pick: self.pick.clone(),
        }
    }
}

impl<T> Deref for UsePendingHandle<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T: PartialEq> PartialEq for UsePendingHandle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value && self.pending == other.pending && self.pick == other.pick
    }
}

impl<T: fmt::Debug> fmt::Debug for UsePendingHandle<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.fmt(f)
    }
}

impl<T: PartialEq + 'static> UsePendingHandle<T> {
    /// The user's choice: shown (and re-rendered) while busy. Become busy in
    /// the same event (e.g. as saving starts), or it's forgotten.
    ///
    /// (It's forgotten when a render sees `busy` false: if `busy` becomes
    /// false and true again before rendering, e.g. as one save ends and
    /// another starts, it's still shown.)
    pub fn set(&self, value: T) {
        self.pick.set(Some(value));
    }

    /// Whether the value shown is the user's choice, not the saved value.
    pub fn is_pending(&self) -> bool {
        self.pending
    }
}

/// The value to show for `saved` (e.g. a value on a server) in a control
/// the user changes: while `busy` (saving a change, until `saved` is up to
/// date), the one the user last chose ([`UsePendingHandle::set`]);
/// otherwise `saved`. A control whose value is applied on every render (a
/// `<select>`'s `selected` options, an `<input>`'s `value`) then shows the
/// user's choice while it's saved, rather than the old value, and whatever
/// was saved after.
///
/// ```ignore
/// let saving = use_state(|| false);
/// let shown = use_pending(&saved_region, *saving);
/// let oninput = {
///     let (shown, saving) = (shown.clone(), saving.clone());
///     Callback::from(move |region| {
///         shown.set(region);
///         saving.set(true);
///         // When done (saved or not), and `saved_region` is up to date:
///         // `saving.set(false)`.
///         save(region);
///     })
/// };
/// // `selected={*shown == region}`
/// ```
///
/// (An extension: yew has no equivalent.)
pub fn use_pending<T: Clone + PartialEq + 'static>(saved: &T, busy: bool) -> UsePendingHandle<T> {
    let pick = use_state_eq(|| None::<T>);
    let pending = busy && pick.is_some();
    {
        // Forgotten when not busy (so as not to show when busy again, with
        // another change). (Setting it to `None` again does nothing.)
        let setter = pick.setter();
        use_effect_with((busy, pick.is_some()), move |&(busy, _)| {
            if !busy {
                setter.set(None);
            }
        });
    }
    UsePendingHandle {
        value: match &*pick {
            Some(pick) if busy => pick.clone(),
            _ => saved.clone(),
        },
        pending,
        pick: pick.setter(),
    }
}
