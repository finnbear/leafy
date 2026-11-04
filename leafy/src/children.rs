//! Typed children: `ChildrenWithProps<C>`, for components that read the
//! properties of their `<C/>` children.

use crate::component::{AnyComp, BaseComponent, PropsOf, VComp};
use crate::vdom::{Html, IntoPropValue, Key};
use std::any::TypeId;
use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;
use std::rc::Rc;

/// A list of children.
#[derive(Clone, PartialEq)]
pub struct ChildrenRenderer<T> {
    children: Vec<T>,
}

impl<T> Default for ChildrenRenderer<T> {
    fn default() -> Self {
        Self {
            children: Vec::new(),
        }
    }
}

impl<T> ChildrenRenderer<T> {
    /// A list of `children`.
    pub fn new(children: Vec<T>) -> Self {
        Self { children }
    }

    /// Whether there are no children.
    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }

    /// The number of children.
    pub fn len(&self) -> usize {
        self.children.len()
    }

    /// Iterates over clones of the children.
    pub fn iter(&self) -> impl Iterator<Item = T> + '_
    where
        T: Clone,
    {
        self.children.iter().cloned()
    }
}

impl<'a, T: Clone> IntoIterator for &'a ChildrenRenderer<T> {
    type Item = T;
    type IntoIter = std::iter::Cloned<std::slice::Iter<'a, T>>;

    fn into_iter(self) -> Self::IntoIter {
        self.children.iter().cloned()
    }
}

impl<T> IntoIterator for ChildrenRenderer<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.children.into_iter()
    }
}

impl<T> fmt::Debug for ChildrenRenderer<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ChildrenRenderer<_>")
    }
}

/// Children that are all `<C/>`.
pub type ChildrenWithProps<C> = ChildrenRenderer<VChild<C>>;

/// A `<C/>` child, whose properties can be read.
pub struct VChild<C: BaseComponent> {
    /// The child's properties (dereferences to `C::Properties`).
    pub props: ChildProps<C>,
    /// The child's key, if any.
    pub key: Option<Key>,
}

impl<C: BaseComponent> Clone for VChild<C> {
    fn clone(&self) -> Self {
        Self {
            props: self.props.clone(),
            key: self.key.clone(),
        }
    }
}

impl<C: BaseComponent> PartialEq for VChild<C> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && *self.props == *other.props
    }
}

impl<C: BaseComponent> VChild<C> {
    /// From `html!`'s output for a `<C/>`.
    #[doc(hidden)]
    pub fn __from_html(html: Html) -> Self {
        let mut out = Vec::new();
        collect(html, &mut out);
        out.pop().unwrap()
    }
}

impl<C: BaseComponent> From<VChild<C>> for Html {
    fn from(c: VChild<C>) -> Self {
        Html::Comp(c.props.0)
    }
}

impl<C: BaseComponent> IntoPropValue<Html> for VChild<C> {
    fn into_prop_value(self) -> Html {
        self.into()
    }
}

impl<C: BaseComponent> IntoPropValue<ChildrenWithProps<C>> for VChild<C> {
    fn into_prop_value(self) -> ChildrenWithProps<C> {
        ChildrenRenderer::new(vec![self])
    }
}

impl<C: BaseComponent> fmt::Debug for VChild<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VChild<{}>", std::any::type_name::<C>())
    }
}

/// The properties of a [`VChild`].
pub struct ChildProps<C: BaseComponent>(Rc<VComp>, PhantomData<C>);

impl<C: BaseComponent> Clone for ChildProps<C> {
    fn clone(&self) -> Self {
        Self(self.0.clone(), PhantomData)
    }
}

impl<C: BaseComponent> Deref for ChildProps<C> {
    type Target = C::Properties;
    fn deref(&self) -> &C::Properties {
        // SAFETY: checked on construction.
        unsafe { &(*(&self.0.props as *const dyn AnyComp as *const PropsOf<C>)).props }
    }
}

fn collect<C: BaseComponent>(html: Html, out: &mut Vec<VChild<C>>) {
    match html {
        Html::Empty => {}
        Html::List(l) => {
            for c in l.children.iter() {
                collect(c.clone(), out);
            }
        }
        Html::Comp(c) if c.type_id == TypeId::of::<PropsOf<C>>() => out.push(VChild {
            key: c.key.clone(),
            props: ChildProps(c, PhantomData),
        }),
        _ => panic!("expected only <{}/> children", std::any::type_name::<C>()),
    }
}

impl<C: BaseComponent> IntoPropValue<ChildrenWithProps<C>> for Html {
    fn into_prop_value(self) -> ChildrenWithProps<C> {
        let mut children = Vec::new();
        collect(self, &mut children);
        ChildrenRenderer { children }
    }
}
