//! Contexts: values provided to all descendants.

use crate::component::{BaseComponent, CompState, Properties, Thunk, current, schedule, use_slot};
use crate::vdom::{Html, IntoPropValue};
use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::{Rc, Weak};

struct ProviderCell<T> {
    value: RefCell<T>,
    subscribers: Subscribers,
}

/// Decides, given the new value, whether a subscriber must re-render.
type Changed = Rc<dyn Fn(&dyn Any) -> bool>;

/// A subscriber: its component, what decides whether it must re-render (if
/// not always), and, for a [`ContextHandle`], whether that still lives.
type Subscriber = (Weak<CompState>, Option<Changed>, Option<Weak<()>>);

#[derive(Default)]
struct Subscribers(RefCell<Vec<Subscriber>>);

fn live((comp, _, handle): &Subscriber) -> bool {
    comp.strong_count() > 0 && handle.as_ref().is_none_or(|h| h.strong_count() > 0)
}

impl Subscribers {
    #[inline(never)]
    fn notify(&self, value: &dyn Any) {
        let subs = self.0.borrow().clone();
        for sub @ (s, changed, _) in &subs {
            if live(sub) && changed.as_ref().is_none_or(|c| c(value)) {
                schedule(s);
            }
        }
        self.0.borrow_mut().retain(live);
    }

    fn subscribe(&self, comp: &Rc<CompState>, changed: Option<Changed>, handle: Option<Weak<()>>) {
        let mut subs = self.0.borrow_mut();
        // Drop those gone, now and then (so it's amortized O(1)), as a value
        // that never changes never notifies.
        if subs.len() >= 8 && subs.len().is_power_of_two() {
            subs.retain(live);
        }
        subs.push((Rc::downgrade(comp), changed, handle));
    }
}

/// Provides `context` to all components in `children`.
pub struct ContextProvider<T>(PhantomData<T>);

/// The properties of a [`ContextProvider`].
#[derive(PartialEq)]
pub struct ContextProviderProps<T: PartialEq> {
    /// The value provided. Subscribers are notified when it changes (by
    /// `PartialEq`).
    pub context: T,
    /// The children, whose components can use the context.
    pub children: Html,
}

#[doc(hidden)]
pub struct ContextProviderPropsBuilder<T, C> {
    context: Option<T>,
    children: Html,
    _c: PhantomData<C>,
}

impl<T: Clone + PartialEq + 'static> Properties for ContextProviderProps<T> {
    type Builder = ContextProviderPropsBuilder<T, ()>;
    #[inline(always)]
    fn builder() -> Self::Builder {
        ContextProviderPropsBuilder {
            context: None,
            children: Html::Empty,
            _c: PhantomData,
        }
    }
}

impl<T, C> ContextProviderPropsBuilder<T, C> {
    #[inline(always)]
    pub fn context(
        self,
        v: impl IntoPropValue<T>,
    ) -> ContextProviderPropsBuilder<T, crate::props::Set> {
        ContextProviderPropsBuilder {
            context: Some(v.into_prop_value()),
            children: self.children,
            _c: PhantomData,
        }
    }

    #[inline(always)]
    pub fn children(mut self, v: impl IntoPropValue<Html>) -> Self {
        self.children = v.into_prop_value();
        self
    }
}

impl<T: PartialEq> crate::component::Build for ContextProviderPropsBuilder<T, crate::props::Set> {
    type Output = ContextProviderProps<T>;
    #[inline(always)]
    fn __build(self) -> ContextProviderProps<T> {
        self.build()
    }
}

impl<T: PartialEq> ContextProviderPropsBuilder<T, crate::props::Set> {
    #[inline(always)]
    pub fn build(self) -> ContextProviderProps<T> {
        ContextProviderProps {
            context: self.context.unwrap(),
            children: self.children,
        }
    }
}

impl<T: Clone + PartialEq + 'static> BaseComponent for ContextProvider<T> {
    type Properties = ContextProviderProps<T>;

    fn __run(props: &Self::Properties, state: &Rc<CompState>) -> Option<Html> {
        let cell = use_slot(Thunk::new(&mut Some(|()| {
            let cell = Rc::new(ProviderCell {
                value: RefCell::new(props.context.clone()),
                subscribers: Subscribers::default(),
            });
            *state.provides.borrow_mut() = Some(cell.clone());
            cell
        })));
        if *cell.value.borrow() != props.context {
            *cell.value.borrow_mut() = props.context.clone();
            cell.subscribers.notify(&props.context);
        }
        Some(props.children.clone())
    }
}

/// Finds the nearest ancestor providing a context of type `type_id`.
#[inline(never)]
fn find_provider(type_id: TypeId) -> Option<Rc<dyn Any>> {
    find_provider_above(&current(), type_id)
}

fn find_provider_above(comp: &CompState, type_id: TypeId) -> Option<Rc<dyn Any>> {
    let mut comp = comp.parent();
    while let Some(c) = comp {
        if let Some(p) = &*c.provides.borrow()
            && (**p).type_id() == type_id
        {
            return Some(p.clone());
        }
        comp = c.parent();
    }
    None
}

/// The value of the nearest ancestor [`ContextProvider<T>`], re-rendering
/// when it changes.
#[inline(never)]
pub fn use_context<T: Clone + PartialEq + 'static>() -> Option<T> {
    let slot = use_slot(Thunk::new(&mut Some(|()| {
        let provider = find_provider(TypeId::of::<ProviderCell<T>>())?;
        let provider = provider.downcast::<ProviderCell<T>>().ok()?;
        provider.subscribers.subscribe(&current(), None, None);
        Some(provider)
    })));
    (*slot).as_ref().map(|p| p.value.borrow().clone())
}

struct SelectorCell<T, S> {
    provider: Option<Rc<ProviderCell<T>>>,
    selected: RefCell<Option<S>>,
    #[allow(clippy::type_complexity)]
    selector: RefCell<Option<Rc<dyn Fn(&T) -> S>>>,
}

/// As [`use_context`], but returns `selector(&context)`, and re-renders only
/// when that changes (not when other parts of the context do).
///
/// (An extension: yew has no equivalent.)
pub fn use_context_selector<T, S, F>(selector: F) -> Option<S>
where
    T: Clone + PartialEq + 'static,
    S: Clone + PartialEq + 'static,
    F: Fn(&T) -> S + 'static,
{
    let cell = use_slot(Thunk::new(&mut Some(|()| {
        let provider = find_provider(TypeId::of::<ProviderCell<T>>())
            .and_then(|p| p.downcast::<ProviderCell<T>>().ok());
        Rc::new(SelectorCell::<T, S> {
            provider,
            selected: RefCell::new(None),
            selector: RefCell::new(None),
        })
    })));
    let provider = cell.provider.as_ref()?;
    if cell.selector.borrow().is_none() {
        let weak = Rc::downgrade(&cell);
        provider.subscribers.subscribe(
            &current(),
            Some(Rc::new(move |value: &dyn Any| {
                let (Some(cell), Some(value)) = (weak.upgrade(), value.downcast_ref::<T>()) else {
                    return false;
                };
                let selector = cell.selector.borrow().clone();
                selector.is_some_and(|s| cell.selected.borrow().as_ref() != Some(&s(value)))
            })),
            None,
        );
    }
    let selected = selector(&provider.value.borrow());
    *cell.selector.borrow_mut() = Some(Rc::new(selector));
    *cell.selected.borrow_mut() = Some(selected.clone());
    Some(selected)
}

/// Keeps a struct component subscribed to a context: see
/// [`Scope::context`](crate::Scope::context).
pub struct ContextHandle<T> {
    _alive: Rc<()>,
    _t: PhantomData<T>,
}

/// The value of the nearest ancestor of `comp` providing a `T`, calling
/// `callback` with the new value when it changes (while the returned handle
/// lives).
pub(crate) fn subscribe_context<T: Clone + PartialEq + 'static>(
    comp: &Rc<CompState>,
    callback: crate::Callback<T>,
) -> Option<(T, ContextHandle<T>)> {
    let provider = find_provider_above(comp, TypeId::of::<ProviderCell<T>>())?
        .downcast::<ProviderCell<T>>()
        .ok()?;
    let alive = Rc::new(());
    provider.subscribers.subscribe(
        comp,
        Some(Rc::new(move |value: &dyn Any| {
            if let Some(value) = value.downcast_ref::<T>() {
                callback.emit(value.clone());
            }
            // (The callback decides whether to re-render.)
            false
        })),
        // (Dropped with the handle.)
        Some(Rc::downgrade(&alive)),
    );
    let value = provider.value.borrow().clone();
    Some((
        value,
        ContextHandle {
            _alive: alive,
            _t: PhantomData,
        },
    ))
}
