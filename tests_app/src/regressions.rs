//! Regression tests for behavior that broke in yew at some point (numbers
//! are yew issues), which leafy gets right.

use super::*;
use crate::issues::{set, use_test_state};
use wasm_bindgen::JsValue;
use web_sys::Event;

fn dispatch(el: &Element, sel: &str, name: &str, bubbles: bool) -> bool {
    let init = web_sys::EventInit::new();
    init.set_bubbles(bubbles);
    init.set_cancelable(true);
    let e = Event::new_with_event_init_dict(name, &init).unwrap();
    q(el, sel).dispatch_event(&e).unwrap()
}

// ------------------------------- re-entrant events (#264, #2989, #3523)

#[function_component(Reentrant)]
fn reentrant() -> Html {
    let n = use_state(|| 0u32);
    let r = use_node_ref();
    let outer = {
        let r = r.clone();
        let n = n.clone();
        Callback::from(move |_: MouseEvent| {
            log("outer");
            // Programmatically click another element, whose listener runs
            // inside this one.
            r.cast::<HtmlElement>().unwrap().click();
            n.set(*n + 1);
        })
    };
    let inner = Callback::from(|_: MouseEvent| log("inner"));
    html! {
        <div>
            <button id="outer" onclick={outer}/>
            <button id="inner" ref={r} onclick={inner}/>
            <i>{*n}</i>
        </div>
    }
}

pub async fn test_reentrant() -> TestResult {
    let el = mount::<Reentrant>(());
    tick().await;
    click(&el, "#outer");
    click(&el, "#outer");
    tick().await;
    eq!(take_log(), vec!["outer", "inner", "outer", "inner"]);
    eq!(q(&el, "i").inner_text(), "1");
    Ok(())
}

// ------------------------------------------- duplicate keys (#3854)

#[function_component(DuplicateKeys)]
fn duplicate_keys() -> Html {
    let s = use_test_state();
    let keys: &[u32] = if s == 0 { &[1, 1, 2] } else { &[2, 1, 1, 1] };
    html! {
        <ul>{for keys.iter().enumerate().map(|(i, k)| html! { <li key={*k}>{i}</li> })}</ul>
    }
}

pub async fn test_duplicate_keys() -> TestResult {
    let el = mount::<DuplicateKeys>(());
    tick().await;
    eq!(html(&el), "<ul><li>0</li><li>1</li><li>2</li></ul>");
    set(1).await;
    eq!(
        html(&el),
        "<ul><li>0</li><li>1</li><li>2</li><li>3</li></ul>"
    );
    Ok(())
}

// ----------------- state set while rendering; batching (#2635, #1034, #1222)

#[derive(Properties, PartialEq)]
pub struct ReportProps {
    on_render: Callback<u32>,
    v: u32,
}

#[function_component(Reporter)]
fn reporter(props: &ReportProps) -> Html {
    // Sets the parent's state during render.
    props.on_render.emit(props.v);
    html! { <b>{props.v}</b> }
}

#[function_component(RenderSetsState)]
fn render_sets_state() -> Html {
    let s = use_test_state();
    let seen = use_state(|| 0u32);
    log(format!("render {s} {}", *seen));
    let on_render = {
        let seen = seen.setter();
        use_callback((), move |v: u32, _| seen.set(v))
    };
    html! { <><Reporter {on_render} v={s}/><i>{*seen}</i></> }
}

enum BMsg {
    Add,
}

struct Batching {
    n: u32,
}

impl Component for Batching {
    type Message = BMsg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        // A message sent from create is handled.
        ctx.link().send_message(BMsg::Add);
        Self { n: 0 }
    }

    fn update(&mut self, _: &Context<Self>, _: BMsg) -> bool {
        self.n += 1;
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        log(format!("view {}", self.n));
        let three = ctx
            .link()
            .batch_callback(|_: MouseEvent| vec![BMsg::Add, BMsg::Add, BMsg::Add]);
        html! { <button id="three" onclick={three}>{self.n}</button> }
    }

    fn rendered(&mut self, _: &Context<Self>, first: bool) {
        log(format!("rendered {first}"));
    }
}

pub async fn test_render_sets_state() -> TestResult {
    let el = mount::<RenderSetsState>(());
    tick().await;
    eq!(take_log(), vec!["render 0 0", "render 0 0"]);
    set(4).await;
    eq!(take_log(), vec!["render 4 0", "render 4 4"]);
    eq!(html(&el), "<b>4</b><i>4</i>");

    let el = mount::<Batching>(());
    tick().await;
    // (The message sent from create is handled before the first view.)
    eq!(take_log(), vec!["view 1", "rendered true"]);
    click(&el, "#three");
    tick().await;
    eq!(take_log(), vec!["view 4", "rendered false"]);
    Ok(())
}

// --------------- propagation and default actions (#934, #2044, #921, #2110)

#[function_component(Propagation)]
fn propagation() -> Html {
    let outer = Callback::from(|_: MouseEvent| log("outer"));
    let stop = Callback::from(|e: MouseEvent| {
        e.stop_propagation();
        log("stop");
    });
    let pass = Callback::from(|_: MouseEvent| log("pass"));
    let prevent = Callback::from(|e: WheelEvent| e.prevent_default());
    let prevent_click = Callback::from(|e: MouseEvent| e.prevent_default());
    let blur = Callback::from(|_: FocusEvent| log("blur"));
    let scroll = Callback::from(|_: Event| log("scroll"));
    let load = Callback::from(|_: Event| log("load"));
    html! {
        <div onclick={outer}>
            <button id="stop" onclick={stop}/>
            <button id="pass" onclick={pass}/>
            <div id="wheel" onwheel={prevent}/>
            <a id="link" href="#nowhere" onclick={prevent_click}/>
            <input id="blur" onblur={blur}/>
            <div id="scroll" onscroll={scroll}/>
            <img id="img" onload={load}/>
        </div>
    }
}

pub async fn test_propagation() -> TestResult {
    let el = mount::<Propagation>(());
    tick().await;
    click(&el, "#stop");
    click(&el, "#pass");
    eq!(take_log(), vec!["stop", "pass", "outer"]);
    // Not canceled: default prevented (the listener isn't passive).
    eq!(dispatch(&el, "#wheel", "wheel", true), false);
    eq!(dispatch(&el, "#link", "click", true), false);
    eq!(take_log(), vec!["outer"]);
    // Events that don't bubble reach their element's listeners.
    q(&el, "#blur").focus().unwrap();
    q(&el, "#blur").blur().unwrap();
    dispatch(&el, "#scroll", "scroll", false);
    dispatch(&el, "#img", "load", false);
    eq!(take_log(), vec!["blur", "scroll", "load"]);
    Ok(())
}

// ------------------- nodes added by other code; VRef (#910, #963, #347)

thread_local! {
    static FOREIGN: RefCell<Option<web_sys::Node>> = const { RefCell::new(None) };
}

#[function_component(Foreign)]
fn foreign() -> Html {
    let s = use_test_state();
    let node = FOREIGN.with_borrow(|n| n.clone()).unwrap();
    html! {
        <div id="host">
            <i>{s}</i>
            if s % 2 == 0 { {Html::VRef(node)} }
            <b/>
        </div>
    }
}

pub async fn test_foreign() -> TestResult {
    let span = document().create_element("span").unwrap();
    span.set_text_content(Some("vref"));
    FOREIGN.with_borrow_mut(|n| *n = Some(span.clone().into()));
    let el = mount::<Foreign>(());
    tick().await;
    eq!(
        html(&el),
        "<div id=\"host\"><i>0</i><span>vref</span><b></b></div>"
    );
    // Other code adds a node to an element leafy manages.
    let extra = document().create_element("u").unwrap();
    q(&el, "#host").append_child(&extra).unwrap();
    set(2).await;
    eq!(
        html(&el),
        "<div id=\"host\"><i>2</i><span>vref</span><b></b><u></u></div>"
    );
    // The VRef node, removed by other code, then by leafy.
    span.remove();
    set(3).await;
    eq!(html(&el), "<div id=\"host\"><i>3</i><b></b><u></u></div>");
    set(4).await;
    eq!(
        html(&el),
        "<div id=\"host\"><i>4</i><span>vref</span><b></b><u></u></div>"
    );
    Ok(())
}

// ----------------- the same Html twice; raw HTML (#3345, #3338, #2841)

#[function_component(Twice)]
fn twice() -> Html {
    let s = use_test_state();
    let item = html! { <b>{s}</b> };
    let raw = Html::from_html_unchecked(AttrValue::from(format!("text {s}<i>i</i>")));
    html! { <p>{item.clone()}{"-"}{item}{raw}</p> }
}

pub async fn test_twice() -> TestResult {
    let el = mount::<Twice>(());
    tick().await;
    eq!(html(&el), "<p><b>0</b>-<b>0</b>text 0<i>i</i></p>");
    set(1).await;
    eq!(html(&el), "<p><b>1</b>-<b>1</b>text 1<i>i</i></p>");
    Ok(())
}

// ------------------- uncontrolled inputs keep their state (#1367, #2875)

#[function_component(Uncontrolled)]
fn uncontrolled() -> Html {
    let s = use_test_state();
    html! {
        <div>
            <input id="text" class={format!("c{s}")}/>
            <input id="box" type="checkbox" class={format!("c{s}")}/>
            <textarea id="area" class={format!("c{s}")}/>
        </div>
    }
}

pub async fn test_uncontrolled() -> TestResult {
    let el = mount::<Uncontrolled>(());
    tick().await;
    let set_prop = |sel: &str, name: &str, v: JsValue| {
        js_sys::Reflect::set(&q(&el, sel), &name.into(), &v).unwrap();
    };
    let get_prop =
        |sel: &str, name: &str| js_sys::Reflect::get(&q(&el, sel), &name.into()).unwrap();
    set_prop("#text", "value", "typed".into());
    set_prop("#box", "checked", JsValue::TRUE);
    set_prop("#area", "value", "area".into());
    set(1).await;
    eq!(
        get_prop("#text", "value").as_string(),
        Some("typed".to_owned())
    );
    eq!(get_prop("#box", "checked").as_bool(), Some(true));
    eq!(
        get_prop("#area", "value").as_string(),
        Some("area".to_owned())
    );
    Ok(())
}

// ------ components switching between types and empty (#494, #2462, #4092)

#[derive(Properties, PartialEq)]
pub struct KindProps {
    kind: u32,
}

#[function_component(Empty)]
fn empty(_: &KindProps) -> Html {
    Html::default()
}

#[function_component(Kind)]
fn kind(props: &KindProps) -> Html {
    match props.kind % 3 {
        0 => html! { <b>{"b"}</b> },
        1 => html! { <><i>{"i"}</i><i>{"j"}</i></> },
        _ => Html::default(),
    }
}

#[function_component(Switching)]
fn switching() -> Html {
    let s = use_test_state();
    html! {
        <div>
            {"<"}
            if s % 2 == 0 { <Kind kind={s}/> } else { <Empty kind={s}/> }
            <Empty kind={s}/>
            <Kind kind={s + 1}/>
            {match s % 3 { 0 => html! { <Kind kind={s}/> }, 1 => html! { <Empty kind={s}/> }, _ => html! { {"t"} } }}
            {">"}
        </div>
    }
}

pub async fn test_switching() -> TestResult {
    let el = mount::<Switching>(());
    tick().await;
    let k = |kind: u32| match kind % 3 {
        0 => "<b>b</b>",
        1 => "<i>i</i><i>j</i>",
        _ => "",
    };
    for s in 0..12 {
        set(s).await;
        let first = if s % 2 == 0 { k(s) } else { "" };
        let last = match s % 3 {
            0 => k(s),
            1 => "",
            _ => "t",
        };
        eq!(
            html(&el),
            format!("<div>&lt;{first}{}{last}&gt;</div>", k(s + 1))
        );
    }
    Ok(())
}
