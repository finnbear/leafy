use crate::component::{__comp, BaseComponent, CompState, mount_root};
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys::Element;

/// Mounts a root component.
pub struct Renderer<C: BaseComponent> {
    root: Element,
    props: C::Properties,
}

impl<C: BaseComponent> Renderer<C>
where
    C::Properties: Default,
{
    /// Renders into `<body>`, with default properties.
    pub fn new() -> Self {
        Self::with_props(Default::default())
    }

    /// Renders into `root`, with default properties.
    pub fn with_root(root: Element) -> Self {
        Self::with_root_and_props(root, Default::default())
    }
}

impl<C: BaseComponent> Default for Renderer<C>
where
    C::Properties: Default,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<C: BaseComponent> Renderer<C> {
    /// Renders into `<body>`, with `props`.
    pub fn with_props(props: C::Properties) -> Self {
        Self::with_root_and_props(crate::dom::body().unchecked_into(), props)
    }

    /// Renders into `root`, with `props`.
    pub fn with_root_and_props(root: Element, props: C::Properties) -> Self {
        Self { root, props }
    }

    /// Mounts the component, replacing the root's contents (as yew).
    ///
    /// First sets the default panic hook, unless [`set_custom_panic_hook`] was
    /// called.
    pub fn render(self) -> AppHandle<C> {
        set_default_panic_hook();
        let state = mount_root(__comp::<C>(None, self.props), self.root.unchecked_ref());
        AppHandle {
            _state: state,
            _c: std::marker::PhantomData,
        }
    }
}

/// A mounted application.
pub struct AppHandle<C> {
    _state: Rc<CompState>,
    _c: std::marker::PhantomData<C>,
}

impl<C> AppHandle<C> {
    /// Unmounts the application.
    pub fn destroy(self) {
        crate::component::unmount_root(&self._state);
    }
}

thread_local! {
    static PANIC_HOOK_IS_SET: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn console_error(s: &str);
    #[wasm_bindgen(js_name = Error)]
    type JsError;
    #[wasm_bindgen(constructor, js_class = Error)]
    fn new() -> JsError;
    #[wasm_bindgen(method, getter)]
    fn stack(this: &JsError) -> String;
}

/// Sets a panic hook, which [`Renderer::render`] will then not replace with
/// its default (which logs the panic and a stack trace to the console).
pub fn set_custom_panic_hook(
    hook: Box<dyn Fn(&std::panic::PanicHookInfo<'_>) + Sync + Send + 'static>,
) {
    std::panic::set_hook(hook);
    PANIC_HOOK_IS_SET.set(true);
}

fn set_default_panic_hook() {
    if !PANIC_HOOK_IS_SET.replace(true) {
        std::panic::set_hook(Box::new(|info| {
            console_error(&format!("{info}\n\nStack:\n\n{}", JsError::new().stack()));
        }));
    }
}
