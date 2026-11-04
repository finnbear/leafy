use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys::Node;

/// A reference to a DOM node rendered with `ref={node_ref}`.
#[derive(Default)]
pub struct NodeRef(Rc<RefCell<Option<Node>>>);

impl Clone for NodeRef {
    #[inline(never)]
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl NodeRef {
    /// The node, if one is rendered with this reference.
    pub fn get(&self) -> Option<Node> {
        self.0.borrow().clone()
    }

    /// The node, if one is rendered with this reference and is a `T`.
    pub fn cast<T: JsCast>(&self) -> Option<T> {
        self.get().and_then(|n| n.dyn_into().ok())
    }

    /// Whether this refers to `node`.
    pub(crate) fn is(&self, node: &Node) -> bool {
        self.0.borrow().as_ref() == Some(node)
    }

    pub(crate) fn set(&self, node: Option<Node>) {
        *self.0.borrow_mut() = node;
    }
}

impl PartialEq for NodeRef {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl fmt::Debug for NodeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NodeRef")
    }
}
