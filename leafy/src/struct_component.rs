//! Struct components, as yew's `Component`.

use crate::component::{
    BaseComponent, CompState, Destroy, PropsOf, Thunk, VComp, on_destroy, push_effect, schedule,
    use_slot,
};
use crate::{Callback, Html, Properties};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::{Rc, Weak};

/// A component with its own state, updated by messages (as yew's
/// `Component`).
pub trait Component: Sized + 'static {
    /// What [`update`](Self::update) handles.
    type Message: 'static;
    /// The component's properties.
    type Properties: Properties;

    /// Creates the component, on its first render.
    fn create(ctx: &Context<Self>) -> Self;

    /// Handles a message, returning whether to re-render (by default, `true`).
    fn update(&mut self, _ctx: &Context<Self>, _msg: Self::Message) -> bool {
        true
    }

    /// Called when the properties change, returning whether to re-render (by
    /// default, `true`).
    fn changed(&mut self, _ctx: &Context<Self>, _old_props: &Self::Properties) -> bool {
        true
    }

    /// Renders the component.
    fn view(&self, ctx: &Context<Self>) -> Html;

    /// Called after each render, once the DOM is updated; `first_render` is
    /// `true` the first time.
    fn rendered(&mut self, _ctx: &Context<Self>, _first_render: bool) {}

    /// Called when the component is removed.
    fn destroy(&mut self, _ctx: &Context<Self>) {}
}

struct StructCell<C: Component> {
    comp: RefCell<Option<C>>,
    messages: RefCell<VecDeque<C::Message>>,
    vcomp: RefCell<Option<Rc<VComp>>>,
    state: Weak<CompState>,
    first: Cell<bool>,
}

struct StructDestroy<C: Component>(Rc<StructCell<C>>);

impl<C: Component> Destroy for StructDestroy<C> {
    fn destroy(&self) {
        if let Some(mut comp) = self.0.comp.take()
            && let Some(ctx) = self.0.context()
        {
            comp.destroy(&ctx);
        }
    }
}

impl<C: Component> StructCell<C> {
    fn context(self: &Rc<Self>) -> Option<Context<C>> {
        let vcomp = self.vcomp.borrow().clone()?;
        Some(Context {
            scope: Scope {
                cell: Rc::downgrade(self),
            },
            vcomp,
        })
    }
}

/// The context of a struct component: its properties and [`Scope`].
pub struct Context<C: Component> {
    scope: Scope<C>,
    vcomp: Rc<VComp>,
}

impl<C: Component> Context<C> {
    /// The component's [`Scope`].
    pub fn link(&self) -> &Scope<C> {
        &self.scope
    }

    /// The current properties.
    pub fn props(&self) -> &C::Properties {
        props_of::<C>(&self.vcomp)
    }
}

fn props_of<C: Component>(vcomp: &VComp) -> &C::Properties {
    assert!(vcomp.type_id == std::any::TypeId::of::<PropsOf<C>>());
    // SAFETY: checked above.
    unsafe { &(*(&vcomp.props as *const dyn crate::component::AnyComp as *const PropsOf<C>)).props }
}

/// Sends messages to a struct component.
pub struct Scope<C: Component> {
    cell: Weak<StructCell<C>>,
}

impl<C: Component> Clone for Scope<C> {
    fn clone(&self) -> Self {
        Self {
            cell: self.cell.clone(),
        }
    }
}

impl<C: Component> Scope<C> {
    /// Queues a message for [`Component::update`] and schedules a render. Does
    /// nothing once the component is destroyed.
    pub fn send_message<T: Into<C::Message>>(&self, msg: T) {
        if let Some(cell) = self.cell.upgrade() {
            cell.messages.borrow_mut().push_back(msg.into());
            schedule(&cell.state);
        }
    }

    /// As [`send_message`](Self::send_message), for several messages.
    pub fn send_message_batch(&self, msgs: Vec<C::Message>) {
        if let Some(cell) = self.cell.upgrade() {
            cell.messages.borrow_mut().extend(msgs);
            schedule(&cell.state);
        }
    }

    /// A callback that sends `f(input)` as a message.
    pub fn callback<F, IN, M>(&self, f: F) -> Callback<IN>
    where
        M: Into<C::Message>,
        F: Fn(IN) -> M + 'static,
    {
        let scope = self.clone();
        Callback::from(move |input| scope.send_message(f(input)))
    }

    /// The value of the nearest ancestor `ContextProvider<T>`, and a handle
    /// which, while kept, has `callback` called with each new value.
    pub fn context<T: Clone + PartialEq + 'static>(
        &self,
        callback: Callback<T>,
    ) -> Option<(T, crate::context::ContextHandle<T>)> {
        let state = self.cell.upgrade()?.state.upgrade()?;
        crate::context::subscribe_context(&state, callback)
    }

    /// A callback that sends the messages `f(input)` returns.
    pub fn batch_callback<F, IN, OUT>(&self, f: F) -> Callback<IN>
    where
        OUT: SendAsMessage<C>,
        F: Fn(IN) -> OUT + 'static,
    {
        let scope = self.clone();
        Callback::from(move |input| f(input).send(&scope))
    }
}

/// What a [`Scope::batch_callback`]'s function returns: messages to send.
pub trait SendAsMessage<C: Component> {
    /// Sends the message(s) to `scope`'s component.
    fn send(self, scope: &Scope<C>);
}

impl<C: Component> SendAsMessage<C> for Option<C::Message> {
    fn send(self, scope: &Scope<C>) {
        if let Some(msg) = self {
            scope.send_message(msg);
        }
    }
}

impl<C: Component> SendAsMessage<C> for Vec<C::Message> {
    fn send(self, scope: &Scope<C>) {
        scope.send_message_batch(self);
    }
}

impl<C: Component> BaseComponent for C {
    type Properties = C::Properties;

    fn __run(_props: &C::Properties, state: &Rc<CompState>) -> Option<Html> {
        let cell = use_slot(Thunk::new(&mut Some(|()| StructCell::<C> {
            comp: RefCell::new(None),
            messages: Default::default(),
            vcomp: RefCell::new(None),
            state: Rc::downgrade(state),
            first: Cell::new(true),
        })));
        let vcomp = state.vcomp.borrow().clone();
        let old = cell.vcomp.replace(Some(vcomp.clone()));
        let ctx = cell.context().unwrap();
        let mut render = false;
        if cell.first.get() {
            let comp = C::create(&ctx);
            *cell.comp.borrow_mut() = Some(comp);
            on_destroy(Rc::new(StructDestroy(cell.clone())));
            render = true;
        } else if let Some(old) = old
            && !Rc::ptr_eq(&old, &vcomp)
        {
            let old_props = props_of::<C>(&old);
            render |= cell
                .comp
                .borrow_mut()
                .as_mut()
                .unwrap()
                .changed(&ctx, old_props);
        }
        loop {
            let msg = cell.messages.borrow_mut().pop_front();
            let Some(msg) = msg else { break };
            let mut comp = cell.comp.borrow_mut();
            render |= comp.as_mut().unwrap().update(&ctx, msg);
        }
        if !render {
            return None;
        }
        let html = cell.comp.borrow().as_ref().unwrap().view(&ctx);
        let first = cell.first.replace(false);
        let cell = cell.clone();
        push_effect(Box::new(move || {
            if let Some(ctx) = cell.context()
                && let Some(comp) = cell.comp.borrow_mut().as_mut()
            {
                comp.rendered(&ctx, first);
            }
        }));
        Some(html)
    }
}
