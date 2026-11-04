//! Tests for bugs found by reviewing yew's issue tracker (numbers are yew
//! issues).

use super::*;
use leafy::virtual_dom::VChild;
use wasm_bindgen::JsValue;

const SVG: &str = "http://www.w3.org/2000/svg";
const XHTML: &str = "http://www.w3.org/1999/xhtml";
const MATHML: &str = "http://www.w3.org/1998/Math/MathML";

fn all(el: &Element, sel: &str) -> Vec<Element> {
    let list = el.query_selector_all(sel).unwrap();
    (0..list.length())
        .map(|i| list.item(i).unwrap().unchecked_into())
        .collect()
}

fn prop(v: &JsValue, name: &str) -> JsValue {
    js_sys::Reflect::get(v, &name.into()).unwrap()
}

thread_local! {
    static SET: RefCell<Option<Callback<u32>>> = const { RefCell::new(None) };
}

/// Sets the state of the component under test (which registered `SET`).
pub(crate) async fn set(v: u32) {
    let cb = SET.with_borrow(|s| s.clone().unwrap());
    cb.emit(v);
    tick().await;
}

#[hook]
pub(crate) fn use_test_state() -> u32 {
    let s = use_state(|| 0u32);
    let setter = s.setter();
    SET.with_borrow_mut(|c| *c = Some(Callback::from(move |v| setter.set(v))));
    *s
}

// ------------------------------------------------- SVG and MathML (#143 ..)

#[function_component(SvgG)]
fn svg_g() -> Html {
    html! {
        <g class="g"><rect width="5" height="5"/><foreignObject width="10" height="10"><div class="fo">{"t"}</div></foreignObject></g>
    }
}

#[function_component(SvgTest)]
fn svg_test() -> Html {
    let s = use_test_state();
    let filter = html! { <filter id="f"><feGaussianBlur stdDeviation="2"/></filter> };
    html! {
        <>
            <svg viewBox="0 0 10 10">
                {for (0..2).map(|i| html! { <circle r="5" cx={i.to_string()}/> })}
                if s == 0 { <rect class="r" width="1" height="1"/> } else { <clipPath id="c"/> }
                <defs>{filter}</defs>
                <SvgG/>
                if s == 0 { <a href="#"><text>{"link"}</text></a> }
                {Html::from_html_unchecked(AttrValue::Static("<circle class=\"raw\"/>"))}
            </svg>
            <math>if s == 0 { <mn>{"1"}</mn> } else { <mi>{"x"}</mi> }</math>
        </>
    }
}

pub async fn test_svg() -> TestResult {
    let el = mount::<SvgTest>(());
    tick().await;
    for _ in [0, 1] {
        let svg = all(&el, "svg *");
        if svg.len() < 8 {
            return Err(format!("only {} svg elements", svg.len()));
        }
        for e in svg {
            let want = if e.get_attribute("class").as_deref() == Some("fo") {
                XHTML
            } else {
                SVG
            };
            eq!(
                (e.local_name(), e.namespace_uri()),
                (e.local_name(), Some(want.to_owned()))
            );
        }
        eq!(
            q(&el, "filter").first_element_child().unwrap().local_name(),
            "feGaussianBlur"
        );
        for e in all(&el, "math *") {
            eq!(
                (e.local_name(), e.namespace_uri()),
                (e.local_name(), Some(MATHML.to_owned()))
            );
        }
        set(1).await;
        eq!(all(&el, "clipPath").len(), 1);
    }
    Ok(())
}

// ------------------------------------------------ NodeRef (#968, #1439, ..)

thread_local! {
    static REFS: RefCell<Vec<NodeRef>> = const { RefCell::new(Vec::new()) };
}

#[function_component(RefTest)]
fn ref_test() -> Html {
    let s = use_test_state();
    let shown = use_node_ref();
    let a = use_node_ref();
    let b = use_node_ref();
    let same = use_node_ref();
    REFS.with_borrow_mut(|r| *r = vec![shown.clone(), a.clone(), b.clone(), same.clone()]);
    html! {
        <div>
            if s == 0 { <div id="shown" ref={shown}/> }
            <span ref={if s == 0 { a } else { b }}/>
            if s == 0 { <i ref={same.clone()}/> } else { <b ref={same}/> }
        </div>
    }
}

pub async fn test_node_ref() -> TestResult {
    let _el = mount::<RefTest>(());
    tick().await;
    let refs = REFS.with_borrow(|r| r.clone());
    let tag = |r: &NodeRef| r.cast::<Element>().map(|e| e.local_name());
    eq!(
        refs.iter().map(tag).collect::<Vec<_>>(),
        vec![
            Some("div".to_owned()),
            Some("span".to_owned()),
            None,
            Some("i".to_owned())
        ]
    );
    set(1).await;
    eq!(
        refs.iter().map(tag).collect::<Vec<_>>(),
        vec![None, None, Some("span".to_owned()), Some("b".to_owned())]
    );
    eq!(refs[3].cast::<Element>().unwrap().is_connected(), true);
    set(0).await;
    eq!(
        refs.iter().map(tag).collect::<Vec<_>>(),
        vec![
            Some("div".to_owned()),
            Some("span".to_owned()),
            None,
            Some("i".to_owned())
        ]
    );
    Ok(())
}

// --------------------------------------------- children (#1262, #3256, ..)

#[derive(Properties, PartialEq)]
pub struct WrapProps {
    #[prop_or_default]
    children: Children,
}

#[function_component(CountChildren)]
fn count_children(props: &WrapProps) -> Html {
    let n = props.children.len();
    html! {
        <ul data-n={n.to_string()}>
            for (i, c) in props.children.iter().enumerate() {
                <li>{i}{":"}{c}</li>
            }
        </ul>
    }
}

#[function_component(CountChildrenRef)]
fn count_children_ref(props: &WrapProps) -> Html {
    let mut out = Vec::new();
    for c in &props.children {
        out.push(html! { <li>{c}</li> });
    }
    html! { <ol>{out}</ol> }
}

#[derive(Properties, PartialEq, Clone)]
pub struct ItemProps {
    v: u32,
}

#[function_component(Item)]
fn item(props: &ItemProps) -> Html {
    html! { <b>{props.v}</b> }
}

#[derive(Properties, PartialEq)]
pub struct ItemsProps {
    #[prop_or_default]
    children: ChildrenWithProps<Item>,
    #[prop_or_default]
    side: Option<VChild<Item>>,
}

#[function_component(Items)]
fn items(props: &ItemsProps) -> Html {
    let sum: u32 = props.children.iter().map(|c| c.props.v).sum();
    html! {
        <p data-n={props.children.len().to_string()} data-sum={sum.to_string()}>
            {for props.children.iter()}
            {props.side.clone()}
        </p>
    }
}

#[function_component(ChildrenTest)]
fn children_test() -> Html {
    let xs = [7u32, 8];
    let nested: VChild<Item> = html_nested! { <Item v={100}/> };
    html! {
        <>
            <CountChildren>
                <b>{"A"}</b>{"B"}
                if false { <b>{"C"}</b> }
                <b>{"D"}</b>
                {for xs.iter().map(|x| html! { <i>{*x}</i> })}
            </CountChildren>
            <CountChildrenRef><b>{"A"}</b><b>{"B"}</b></CountChildrenRef>
            <Items side={nested}>
                <Item v={1}/>
                <Item v={2}/>
                if false { <Item v={3}/> }
                {for xs.iter().map(|&v| html_nested! { <Item {v}/> })}
            </Items>
        </>
    }
}

pub async fn test_children() -> TestResult {
    let el = mount::<ChildrenTest>(());
    tick().await;
    let ul = q(&el, "ul");
    eq!(ul.get_attribute("data-n"), Some("6".to_owned()));
    eq!(
        html(&ul),
        "<li>0:<b>A</b></li><li>1:B</li><li>2:</li><li>3:<b>D</b></li><li>4:<i>7</i></li><li>5:<i>8</i></li>"
    );
    eq!(html(&q(&el, "ol")), "<li><b>A</b></li><li><b>B</b></li>");
    let p = q(&el, "p");
    eq!(p.get_attribute("data-n"), Some("4".to_owned()));
    eq!(p.get_attribute("data-sum"), Some("18".to_owned()));
    eq!(html(&p), "<b>1</b><b>2</b><b>7</b><b>8</b><b>100</b>");
    Ok(())
}

// -------------------------------------------------- effect order (#1358)

#[derive(Properties, PartialEq)]
pub struct NameProps {
    name: &'static str,
    #[prop_or_default]
    children: Children,
}

#[function_component(Logger)]
fn logger(props: &NameProps) -> Html {
    let name = props.name;
    let r = use_node_ref();
    {
        let r = r.clone();
        use_effect(move || log(format!("{name} {}", r.get().is_some())));
    }
    html! { <div ref={r}>{props.children.clone()}</div> }
}

struct StructLogger;

impl Component for StructLogger {
    type Message = ();
    type Properties = ();
    fn create(_: &Context<Self>) -> Self {
        Self
    }
    fn view(&self, _: &Context<Self>) -> Html {
        html! { <Logger name="s-child"/> }
    }
    fn rendered(&mut self, _: &Context<Self>, first: bool) {
        log(format!("struct {first}"));
    }
}

#[function_component(EffectOrder)]
fn effect_order() -> Html {
    let s = use_test_state();
    html! {
        <Logger name="root">
            <Logger name="a"><Logger name="a1"/></Logger>
            <Logger name="b"/>
            <StructLogger/>
            {s}
        </Logger>
    }
}

pub async fn test_effect_order() -> TestResult {
    let _el = mount::<EffectOrder>(());
    tick().await;
    eq!(
        take_log(),
        vec![
            "a1 true",
            "a true",
            "b true",
            "s-child true",
            "struct true",
            "root true"
        ]
    );
    set(1).await;
    // The children's properties are equal, so only the root renders.
    eq!(take_log(), vec!["root true"]);
    Ok(())
}

// ------------------------------------------------ keyed list moves (#2990)

#[function_component(KeyedMoves)]
fn keyed_moves() -> Html {
    let s = use_test_state();
    let order: &[u32] = match s {
        0 => &[0, 1, 2, 3, 4],
        1 => &[4, 1, 2, 3, 0],
        2 => &[1, 2, 3, 4, 0],
        _ => &[0, 4, 3, 2, 1],
    };
    html! {
        <ul>
            for i in order.iter() {
                <li key={*i} id={format!("k{i}")}><input/></li>
            }
        </ul>
    }
}

thread_local! {
    static MUTATIONS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

pub async fn test_keyed_moves() -> TestResult {
    let el = mount::<KeyedMoves>(());
    tick().await;
    let ul = q(&el, "ul");
    // Records the ids of the items added (or moved).
    let observer = web_sys::MutationObserver::new(
        wasm_bindgen::closure::Closure::<dyn Fn(js_sys::Array)>::new(|records: js_sys::Array| {
            for r in records.iter() {
                let added = r.unchecked_into::<web_sys::MutationRecord>().added_nodes();
                for i in 0..added.length() {
                    let id = added.item(i).unwrap().unchecked_into::<Element>().id();
                    MUTATIONS.with_borrow_mut(|m| m.push(id));
                }
            }
        })
        .into_js_value()
        .unchecked_ref(),
    )
    .unwrap();
    let opts = web_sys::MutationObserverInit::new();
    opts.set_child_list(true);
    observer.observe_with_options(&ul, &opts).unwrap();
    let take = |_: &web_sys::MutationObserver| {
        let mut m = MUTATIONS.take();
        m.sort();
        m
    };
    let ids = |ul: &HtmlElement| {
        all(ul, "li")
            .iter()
            .map(|e| e.id())
            .collect::<Vec<_>>()
            .join(",")
    };
    q(&el, "#k2 input").focus().unwrap();
    set(1).await;
    eq!(ids(&ul), "k4,k1,k2,k3,k0");
    eq!(take(&observer), vec!["k0", "k4"]);
    let active = document().active_element().unwrap();
    eq!(active.parent_element().unwrap().id(), "k2");
    set(2).await;
    eq!(ids(&ul), "k1,k2,k3,k4,k0");
    eq!(take(&observer), vec!["k4"]);
    set(3).await;
    eq!(ids(&ul), "k0,k4,k3,k2,k1");
    eq!(take(&observer).len(), 4);
    Ok(())
}

// --------------------------------------------- keyed fragments (#36, #1444)

#[function_component(KeyedFragments)]
fn keyed_fragments() -> Html {
    let s = use_test_state();
    let ids: &[u32] = if s == 0 { &[1, 2, 3] } else { &[3, 1, 2] };
    html! {
        <dl>
            {for ids.iter().map(|id| html! { <key={*id}><dt id={format!("t{id}")}>{id}</dt><dd>{"d"}{id}</dd></> })}
        </dl>
    }
}

pub async fn test_keyed_fragments() -> TestResult {
    let el = mount::<KeyedFragments>(());
    tick().await;
    let dt1 = q(&el, "#t1");
    eq!(
        html(&q(&el, "dl")),
        r#"<dt id="t1">1</dt><dd>d1</dd><dt id="t2">2</dt><dd>d2</dd><dt id="t3">3</dt><dd>d3</dd>"#
    );
    set(1).await;
    eq!(
        html(&q(&el, "dl")),
        r#"<dt id="t3">3</dt><dd>d3</dd><dt id="t1">1</dt><dd>d1</dd><dt id="t2">2</dt><dd>d2</dd>"#
    );
    eq!(dt1 == q(&el, "#t1"), true);
    Ok(())
}

// ------------------------------------------- attributes (#3189, #1649, ..)

#[function_component(Attrs)]
fn attrs() -> Html {
    let s = use_test_state();
    let on = s == 1;
    html! {
        <div>
            <my-el id="c1" checked={on}/>
            <my-el id="c2" checked=false/>
            <input id="range" type="range" min=-5 max=5/>
            <video id="v1" muted=true/>
            <video id="v2" muted={on}/>
            <select id="sel">
                <option value="0" selected={s == 0}>{"0"}</option>
                <option value="1" selected={s == 1}>{"1"}</option>
                <option value="2" selected={s == 2}>{"2"}</option>
            </select>
            <textarea id="ta" value={format!("v{s}")}/>
            <p>{ /* nothing */ }</p>
        </div>
    }
}

pub async fn test_attrs() -> TestResult {
    let el = mount::<Attrs>(());
    tick().await;
    eq!(q(&el, "#c1").has_attribute("checked"), false);
    eq!(q(&el, "#c2").has_attribute("checked"), false);
    eq!(q(&el, "#range").get_attribute("min"), Some("-5".to_owned()));
    eq!(prop(&q(&el, "#v1"), "muted").as_bool(), Some(true));
    eq!(prop(&q(&el, "#v2"), "muted").as_bool(), Some(false));
    eq!(
        prop(&q(&el, "#sel"), "value").as_string(),
        Some("0".to_owned())
    );
    eq!(
        prop(&q(&el, "#ta"), "value").as_string(),
        Some("v0".to_owned())
    );
    eq!(html(&q(&el, "p")), "");
    set(1).await;
    eq!(q(&el, "#c1").has_attribute("checked"), true);
    eq!(prop(&q(&el, "#v2"), "muted").as_bool(), Some(true));
    eq!(
        prop(&q(&el, "#sel"), "value").as_string(),
        Some("1".to_owned())
    );
    // The user changes things; the next render applies its values again.
    js_sys::Reflect::set(&q(&el, "#sel"), &"value".into(), &"0".into()).unwrap();
    js_sys::Reflect::set(&q(&el, "#v2"), &"muted".into(), &JsValue::FALSE).unwrap();
    js_sys::Reflect::set(&q(&el, "#ta"), &"value".into(), &"typed".into()).unwrap();
    set(1).await;
    eq!(
        prop(&q(&el, "#sel"), "value").as_string(),
        Some("1".to_owned())
    );
    eq!(prop(&q(&el, "#v2"), "muted").as_bool(), Some(true));
    eq!(
        prop(&q(&el, "#ta"), "value").as_string(),
        Some("v1".to_owned())
    );
    set(2).await;
    eq!(
        prop(&q(&el, "#sel"), "value").as_string(),
        Some("2".to_owned())
    );
    Ok(())
}

// --------------------------------------------- struct context (#2280)

#[derive(Clone, PartialEq)]
struct Ctx(u32);

struct CtxStruct {
    value: Ctx,
    _handle: Option<leafy::ContextHandle<Ctx>>,
}

impl Component for CtxStruct {
    type Message = Ctx;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        let (value, handle) = ctx
            .link()
            .context::<Ctx>(ctx.link().callback(|c: Ctx| c))
            .unwrap();
        Self {
            value,
            _handle: Some(handle),
        }
    }

    fn update(&mut self, _: &Context<Self>, msg: Ctx) -> bool {
        self.value = msg;
        true
    }

    fn view(&self, _: &Context<Self>) -> Html {
        html! { <i>{self.value.0}</i> }
    }
}

#[function_component(CtxStructHost)]
fn ctx_struct_host() -> Html {
    let s = use_test_state();
    html! {
        <ContextProvider<Ctx> context={Ctx(s)}>
            <CtxStruct/>
        </ContextProvider<Ctx>>
    }
}

pub async fn test_struct_context() -> TestResult {
    let el = mount::<CtxStructHost>(());
    tick().await;
    eq!(html(&el), "<i>0</i>");
    set(5).await;
    eq!(html(&el), "<i>5</i>");
    Ok(())
}

// ------------------------------------------------ misc parsing

#[function_component(Parsing)]
fn parsing() -> Html {
    html! {
        <div>
            <a href="#"><img src="data:,"></a>
            <p>{"x"}<br></p>
            <object><param name="a" value="b"></object>
        </div>
    }
}

pub async fn test_parsing() -> TestResult {
    let el = mount::<Parsing>(());
    tick().await;
    eq!(
        html(&el),
        r##"<div><a href="#"><img src="data:,"></a><p>x<br></p><object><param name="a" value="b"></object></div>"##
    );
    Ok(())
}

// ------------------------------------------------------- css!, global_css!

#[function_component(Styled)]
fn styled() -> Html {
    let s = use_test_state();
    let red = css!("color: rgb(255, 0, 0); & > b { font-weight: 700; }");
    global_css!(".styled-global { margin-left: 3px; }").register();
    html! {
        <p id="styled" class={classes!("styled-global", (s == 0).then_some(red))}><b>{s}</b></p>
    }
}

pub async fn test_css() -> TestResult {
    let el = mount::<Styled>(());
    tick().await;
    let styles = || {
        document()
            .head()
            .unwrap()
            .query_selector_all("style")
            .unwrap()
            .length()
    };
    let before = styles();
    let computed = |sel: &str, prop: &str| {
        web_sys::window()
            .unwrap()
            .get_computed_style(&q(&el, sel))
            .unwrap()
            .unwrap()
            .get_property_value(prop)
            .unwrap()
    };
    eq!(computed("#styled", "color"), "rgb(255, 0, 0)");
    eq!(computed("#styled", "margin-left"), "3px");
    eq!(computed("#styled b", "font-weight"), "700");
    let class = q(&el, "#styled").get_attribute("class").unwrap();
    eq!(class.starts_with("styled-global sc-"), true);
    set(1).await;
    eq!(computed("#styled", "color") == "rgb(255, 0, 0)", false);
    set(0).await;
    // Each stylesheet was added once.
    eq!(styles(), before);
    Ok(())
}

// Two `css!` (and two `global_css!`) of the same rules: one `<style>` each,
// as on the server (and as stylist does).
#[function_component(SameRules)]
fn same_rules() -> Html {
    let a = css!("color: rgb(0, 0, 255);");
    let b = css!("color: rgb(0, 0, 255);");
    global_css!(".same-rules { margin-left: 2px; }").register();
    global_css!(".same-rules { margin-left: 2px; }").register();
    html! { <><i id="sa" class={classes!("same-rules", a)}/><i id="sb" class={b}/></> }
}

pub async fn test_css_same_rules() -> TestResult {
    let styles = || {
        document()
            .head()
            .unwrap()
            .query_selector_all("style")
            .unwrap()
            .length()
    };
    let before = styles();
    let el = mount::<SameRules>(());
    tick().await;
    eq!(styles(), before + 2);
    let class = |sel: &str| q(&el, sel).get_attribute("class").unwrap();
    eq!(class("#sa").ends_with(&class("#sb")), true);
    let computed = |sel: &str, prop: &str| {
        web_sys::window()
            .unwrap()
            .get_computed_style(&q(&el, sel))
            .unwrap()
            .unwrap()
            .get_property_value(prop)
            .unwrap()
    };
    eq!(computed("#sa", "color"), "rgb(0, 0, 255)");
    eq!(computed("#sb", "color"), "rgb(0, 0, 255)");
    eq!(computed("#sa", "margin-left"), "2px");
    Ok(())
}
