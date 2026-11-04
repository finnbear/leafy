//! Regression tests for bugs found by code review.

use super::*;
use crate::issues::{set, use_test_state};
use wasm_bindgen::JsCast;

thread_local! {
    static NODE: RefCell<Option<web_sys::Node>> = const { RefCell::new(None) };
}
fn node() -> web_sys::Node {
    NODE.with_borrow_mut(|n| {
        n.get_or_insert_with(|| {
            let s = document().create_element("span").unwrap();
            s.set_text_content(Some("vref"));
            s.into()
        })
        .clone()
    })
}

#[function_component(VrefShape)]
fn vref_shape() -> Html {
    let s = use_test_state();
    if s == 0 {
        Html::VRef(node())
    } else {
        html! { <div>{Html::VRef(node())}</div> }
    }
}

pub async fn test_vref_shape() -> TestResult {
    let el = mount::<VrefShape>(());
    tick().await;
    eq!(html(&el), "<span>vref</span>");
    set(1).await;
    eq!(html(&el), "<div><span>vref</span></div>");
    Ok(())
}

#[function_component(VrefList)]
fn vref_list() -> Html {
    let s = use_test_state();
    let items: Vec<Html> = if s == 0 {
        vec![Html::VRef(node())]
    } else {
        vec![Html::from("a"), Html::VRef(node())]
    };
    html! { <p>{for items}</p> }
}

pub async fn test_vref_list() -> TestResult {
    let el = mount::<VrefList>(());
    tick().await;
    eq!(html(&el), "<p><span>vref</span></p>");
    set(1).await;
    eq!(html(&el), "<p>a<span>vref</span></p>");
    Ok(())
}

#[function_component(BlurRemoved)]
fn blur_removed() -> Html {
    let s = use_test_state();
    let cb = Callback::from(|_: FocusEvent| log("leafyblur"));
    html! { <div>if s == 0 { <input id="bi" onblur={cb}/> }</div> }
}

pub async fn test_blur_removed() -> TestResult {
    let errs =
        wasm_bindgen::closure::Closure::<dyn Fn(web_sys::Event)>::new(|e: web_sys::Event| {
            let m = js_sys::Reflect::get(&e, &"message".into()).unwrap();
            log(format!("error: {:?}", m.as_string()));
        });
    web_sys::window()
        .unwrap()
        .add_event_listener_with_callback("error", errs.as_ref().unchecked_ref())
        .unwrap();
    errs.forget();
    let el = mount::<BlurRemoved>(());
    tick().await;
    let raw = wasm_bindgen::closure::Closure::<dyn Fn()>::new(|| log("rawblur"));
    q(&el, "#bi")
        .add_event_listener_with_callback("blur", raw.as_ref().unchecked_ref())
        .unwrap();
    raw.forget();
    q(&el, "#bi").focus().unwrap();
    set(1).await;
    // The browser fires `blur` as the focused input is removed: leafy's
    // listener is already removed (and its closure freed), so no error.
    eq!(take_log(), vec!["rawblur"]);
    Ok(())
}
#[derive(Properties, PartialEq)]
struct KP {
    hide: Callback<()>,
}
#[function_component(PKid)]
fn pkid(p: &KP) -> Html {
    use_effect_with((), |_| {
        log("kid effect");
        || log("kid cleanup")
    });
    p.hide.emit(());
    html! { <b/> }
}
#[function_component(PPar)]
fn ppar() -> Html {
    let show = use_state(|| true);
    let hide = {
        let s = show.setter();
        use_callback((), move |_: (), _| s.set(false))
    };
    html! { if *show { <PKid {hide}/> } }
}
pub async fn test_dead_effect() -> TestResult {
    let el = mount::<PPar>(());
    tick().await;
    eq!(html(&el), "");
    eq!(take_log(), Vec::<String>::new());
    Ok(())
}
#[function_component(PTwice)]
fn ptwice() -> Html {
    let n = use_state(|| 0u32);
    if *n == 0 {
        n.set(1);
    }
    use_effect_with(*n, |n| {
        log(format!("eff {n}"));
        let n = *n;
        move || log(format!("clean {n}"))
    });
    html! {}
}
pub async fn test_stale_effect() -> TestResult {
    let _el = mount::<PTwice>(());
    tick().await;
    eq!(take_log(), vec!["eff 1".to_string()]);
    Ok(())
}

// Raw HTML in a top-level hole of a template parsed in an `<svg>`.
#[function_component(RawG)]
fn raw_g() -> Html {
    html! { <>{Html::from_html_unchecked(AttrValue::Static("<circle class=\"rawc\"/>"))}<rect/></> }
}

#[function_component(RawSvg)]
fn raw_svg() -> Html {
    html! { <svg><RawG/></svg> }
}

pub async fn test_raw_svg() -> TestResult {
    let el = mount::<RawSvg>(());
    tick().await;
    eq!(
        q(&el, ".rawc").namespace_uri(),
        Some("http://www.w3.org/2000/svg".to_owned())
    );
    Ok(())
}

// Markup the HTML parser restructures (or that it parses specially). (The
// warnings that it's split, allowed.)
#[allow(deprecated)]
#[function_component(Parser)]
fn parser() -> Html {
    let s = use_test_state();
    let x = format!("v{s}");
    html! {
        <div>
            <pre id="pre">{"\n"}<b>{x.clone()}</b></pre>
            <table><tbody><tr>{"\u{a0}"}<td id="td">{x.clone()}</td></tr></tbody></table>
            <p id="p"><li class={x.clone()}></li></p>
            <ul><li><div><li id="li2" class={x.clone()}/></div></li></ul>
            <dl><dt><dd id="dd" class={x.clone()}/></dt></dl>
            <svg><foreignObject><p><div id="fo" class={x.clone()}/></p></foreignObject></svg>
            <svg><style id="svgstyle">{"b{}"}</style><title id="svgtitle">{x.clone()}{"!"}</title></svg>
        </div>
    }
}

pub async fn test_parser() -> TestResult {
    let el = mount::<Parser>(());
    tick().await;
    for s in ["v0", "v1"] {
        eq!(q(&el, "#pre b").text_content(), Some(s.to_owned()));
        eq!(q(&el, "#td").text_content(), Some(s.to_owned()));
        eq!(q(&el, "li:not([id])").class_name(), s);
        eq!(q(&el, "#li2").class_name(), s);
        eq!(q(&el, "#dd").class_name(), s);
        eq!(q(&el, "#fo").class_name(), s);
        eq!(q(&el, "#svgtitle").text_content(), Some(format!("{s}!")));
        set(1).await;
    }
    eq!(q(&el, "#svgstyle").text_content(), Some("b{}".to_owned()));
    Ok(())
}

// A range input's value is applied after its `min` and `max` (as yew does),
// or it's clamped to the default range, 0 to 100. (The value was set first.)
#[function_component(Range)]
fn range() -> Html {
    let s = use_test_state();
    let (min, max, value) = if s == 0 {
        (64, 512, 256)
    } else {
        (200, 1000, 900)
    };
    html! {
        <div>
            <input id="range" type="range" min={min.to_string()} max={max.to_string()} value={value.to_string()}/>
            <input id="check" type="checkbox" disabled={s == 1} checked={s == 1}/>
        </div>
    }
}

pub async fn test_range_value() -> TestResult {
    let el = mount::<Range>(());
    tick().await;
    let input = |id| q(&el, id).dyn_into::<web_sys::HtmlInputElement>().unwrap();
    eq!(input("#range").value(), "256");
    eq!(input("#check").checked(), false);
    // Changed together with the range.
    set(1).await;
    eq!(input("#range").value(), "900");
    eq!(input("#check").checked(), true);
    Ok(())
}

// A camelCase SVG element written as is (yew needed `<@{"clipPath"}>`): the
// HTML parser keeps its case, in the SVG namespace. And optional children.
#[derive(Properties, PartialEq)]
struct MaybeProps {
    #[prop_or_default]
    children: Option<Children>,
}

#[function_component(Maybe)]
fn maybe(p: &MaybeProps) -> Html {
    match &p.children {
        Some(children) => html! { <b id="some">{children.clone()}</b> },
        None => html! { <i id="none"/> },
    }
}

#[function_component(SvgCase)]
fn svg_case() -> Html {
    html! {
        <div>
            <svg><defs><clipPath id="clip" clipPathUnits="userSpaceOnUse"><path d="M 0,0"/></clipPath></defs></svg>
            <Maybe>{"x"}</Maybe>
            <Maybe/>
        </div>
    }
}

pub async fn test_svg_case() -> TestResult {
    let el = mount::<SvgCase>(());
    tick().await;
    let clip = q(&el, "#clip");
    eq!(clip.tag_name(), "clipPath");
    eq!(
        clip.namespace_uri(),
        Some("http://www.w3.org/2000/svg".to_owned())
    );
    eq!(
        clip.get_attribute("clipPathUnits"),
        Some("userSpaceOnUse".to_owned())
    );
    eq!(q(&el, "#some").text_content(), Some("x".to_owned()));
    eq!(q(&el, "#none").tag_name(), "I");
    Ok(())
}

// A component whose root is an element of both HTML and SVG (`<a>`,
// `<title>`) is in its parent's namespace, as in yew: in SVG, directly or
// through a template's top-level hole, it's SVG.
const SVG_NS: &str = "http://www.w3.org/2000/svg";
const HTML_NS: &str = "http://www.w3.org/1999/xhtml";

#[derive(Properties, PartialEq)]
pub struct LinkProps {
    class: AttrValue,
    #[prop_or_default]
    children: Children,
}

#[function_component(Link)]
fn link(props: &LinkProps) -> Html {
    html! { <a href="#l" class={props.class.clone()}>{props.children.clone()}</a> }
}

#[function_component(Tip)]
fn tip() -> Html {
    let s = use_test_state();
    html! { <title class="tip">{format!("tip {s}")}</title> }
}

// Two links and text at its top level (so they're mounted in its fragment).
#[function_component(Links)]
fn links() -> Html {
    html! { <><Link class="l2"><rect class="r2" width="5" height="5"/></Link>{"x"}<Link class="l3"/></> }
}

// A keyed element first (built as a template of its own), then an `<a>`.
#[function_component(KeyedFirst)]
fn keyed_first() -> Html {
    let k = 1;
    html! { <><span key={k}/><a class="l6" href="#"/></> }
}

// A keyed `<a>` first, then an SVG element: SVG.
#[function_component(KeyedAFirst)]
fn keyed_a_first() -> Html {
    let k = 1;
    html! { <><a key={k} class="l9" href="#"/><circle class="c9" r="1"/></> }
}

// A keyed SVG element first: the rest are SVG too.
#[function_component(KeyedSvgFirst)]
fn keyed_svg_first() -> Html {
    let k = 1;
    html! { <><rect key={k}/><a class="l7" href="#"><span/></a></> }
}

#[function_component(Ambiguous)]
fn ambiguous() -> Html {
    html! {
        <div>
            <svg>
                <Link class="l1"><circle class="c1" r="5"/></Link>
                <g><Links/></g>
                <g><KeyedFirst/></g>
                <g><KeyedSvgFirst/></g>
                <g><KeyedAFirst/></g>
                // (An SVG `<title>`'s content is HTML, as the parser parses it.)
                <title><Link class="l8"/></title>
                <Tip/>
                <foreignObject width="10" height="10"><Link class="l4"/></foreignObject>
            </svg>
            <Link class="l5"><span class="s5">{"html"}</span></Link>
        </div>
    }
}

pub async fn test_ambiguous_svg() -> TestResult {
    let el = mount::<Ambiguous>(());
    tick().await;
    let ns = |sel: &str| q(&el, sel).namespace_uri();
    for sel in [
        ".l1", ".c1", ".l2", ".r2", ".l3", ".l6", ".l7", ".l9", ".c9", ".tip",
    ] {
        eq!((sel, ns(sel)), (sel, Some(SVG_NS.to_owned())));
    }
    for sel in [".l4", ".l5", ".s5", ".l8"] {
        eq!((sel, ns(sel)), (sel, Some(HTML_NS.to_owned())));
    }
    // It's rendered (an HTML `<a>` in an `<svg>` isn't, nor is its content).
    let circle: web_sys::Element = q(&el, ".c1").unchecked_into();
    eq!(circle.get_bounding_client_rect().width(), 10.0);
    eq!(q(&el, ".tip").text_content(), Some("tip 0".to_owned()));
    set(1).await;
    eq!(q(&el, ".tip").text_content(), Some("tip 1".to_owned()));
    Ok(())
}

// A `key` on an element other than a root: when it changes, the element is
// replaced (as in yew).
#[function_component(NestedKey)]
fn nested_key() -> Html {
    let s = use_test_state();
    let key = s / 2;
    html! {
        <div>
            <span id="k1" key={key} class={format!("c{s}")}>{s}</span>
            <>
                <b id="k2" key={key}/>
                <i id="k3"/>
            </>
            <p><RootKeys k={key}/></p>
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct RootKeysProps {
    k: u32,
}

#[function_component(RootKeys)]
fn root_keys(props: &RootKeysProps) -> Html {
    html! { <><b id="k4" key={props.k}/><i id="k5"/></> }
}

pub async fn test_nested_key() -> TestResult {
    let el = mount::<NestedKey>(());
    tick().await;
    let mark = |el: &Element, sel: &str| {
        js_sys::Reflect::set(&q(el, sel), &"mark".into(), &true.into()).unwrap();
    };
    let marked = |el: &Element, sel: &str| {
        js_sys::Reflect::get(&q(el, sel), &"mark".into())
            .unwrap()
            .is_truthy()
    };
    for sel in ["#k1", "#k2", "#k3"] {
        mark(&el, sel);
    }
    for sel in ["#k4", "#k5"] {
        mark(&el, sel);
    }
    // Same key: kept, and patched.
    set(1).await;
    eq!(
        html(&el),
        r#"<div><span id="k1" class="c1">1</span><b id="k2"></b><i id="k3"></i><p><b id="k4"></b><i id="k5"></i></p></div>"#
    );
    eq!(
        [marked(&el, "#k1"), marked(&el, "#k2"), marked(&el, "#k3")],
        [true; 3]
    );
    eq!([marked(&el, "#k4"), marked(&el, "#k5")], [true; 2]);
    // Another key: replaced.
    set(2).await;
    eq!(
        html(&el),
        r#"<div><span id="k1" class="c2">2</span><b id="k2"></b><i id="k3"></i><p><b id="k4"></b><i id="k5"></i></p></div>"#
    );
    eq!(
        [marked(&el, "#k1"), marked(&el, "#k2"), marked(&el, "#k3")],
        [false, false, true]
    );
    eq!([marked(&el, "#k4"), marked(&el, "#k5")], [false, true]);
    Ok(())
}

// `use_pending`: a controlled `<select>` (whose `selected` is applied on
// every render) shows the user's last choice while it's saved (`busy`), and
// what was saved after (whether that's the choice, a normalized one, or the
// old value, as the save failed).
thread_local! {
    static SAVED: RefCell<Option<UseStateHandle<&'static str>>> = const { RefCell::new(None) };
    static BUSY: RefCell<Option<UseStateHandle<bool>>> = const { RefCell::new(None) };
}

#[function_component(Pending)]
fn pending() -> Html {
    // Re-rendered by other state too (as by a game, every frame).
    let s = use_test_state();
    let saved = use_state(|| "a");
    let busy = use_state(|| false);
    let shown = use_pending(&*saved, *busy);
    SAVED.with_borrow_mut(|h| *h = Some(saved.clone()));
    BUSY.with_borrow_mut(|h| *h = Some(busy.clone()));
    let oninput = {
        let shown = shown.clone();
        Callback::from(move |e: InputEvent| {
            let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
            shown.set(match select.value().as_str() {
                "a" => "a",
                "b" => "b",
                _ => "c",
            });
            // (Saved later, by the test.)
            busy.set(true);
        })
    };
    html! {
        <div class={format!("c{s}")} data-pending={shown.is_pending().to_string()}>
            <select id="sel" {oninput}>
                { for ["a", "b", "c"].into_iter().map(|o| html! {
                    <option value={o} selected={*shown == o}>{o}</option>
                }) }
            </select>
        </div>
    }
}

pub async fn test_pending() -> TestResult {
    let el = mount::<Pending>(());
    tick().await;
    let sel: web_sys::HtmlSelectElement = q(&el, "#sel").unchecked_into();
    let state = || {
        let pending = q(&el, "div").get_attribute("data-pending").unwrap();
        (sel.value(), pending)
    };
    let is = |v: &str, p: bool| (v.to_owned(), p.to_string());
    let saved = |v| SAVED.with_borrow(|h| h.clone().unwrap().set(v));
    let busy = |b| BUSY.with_borrow(|h| h.clone().unwrap().set(b));
    eq!(state(), is("a", false));
    // The user's choice, being saved: shown through other renders.
    input(&el, "#sel", "b");
    tick().await;
    set(1).await;
    eq!(state(), is("b", true));
    // Another, then back to the first: the last is shown while saving, even
    // as the others save.
    input(&el, "#sel", "c");
    input(&el, "#sel", "a");
    tick().await;
    saved("b");
    tick().await;
    eq!(state(), is("a", true));
    saved("c");
    set(2).await;
    eq!(state(), is("a", true));
    // Done: what was saved.
    saved("a");
    busy(false);
    tick().await;
    eq!(state(), is("a", false));
    // Saved as another value (normalized, or by someone else): that.
    input(&el, "#sel", "b");
    tick().await;
    eq!(state(), is("b", true));
    saved("c");
    busy(false);
    tick().await;
    eq!(state(), is("c", false));
    // Failed: the saved value.
    input(&el, "#sel", "b");
    tick().await;
    busy(false);
    tick().await;
    eq!(state(), is("c", false));
    // Busy again (with another change): the old choice isn't shown.
    busy(true);
    tick().await;
    eq!(state(), is("c", false));
    busy(false);
    tick().await;
    Ok(())
}
