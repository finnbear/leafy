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
