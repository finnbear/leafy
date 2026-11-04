//! Browser tests for leafy. Build with trunk and open (or run
//! `../tests_app/run.sh`); results are logged and summarized in the title.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use wasm_bindgen::JsCast;

use leafy::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Element, HtmlElement, HtmlInputElement};

// ----------------------------------------------------------------- harness

fn document() -> web_sys::Document {
    web_sys::window().unwrap().document().unwrap()
}

async fn tick() {
    let p = js_sys::Promise::new(&mut |res, _| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback(&res)
            .unwrap();
    });
    JsFuture::from(p).await.unwrap();
}

fn container() -> Element {
    let el = document().create_element("div").unwrap();
    document().body().unwrap().append_child(&el).unwrap();
    el
}

/// `innerHTML` without comments.
fn html(el: &Element) -> String {
    let s = el.inner_html();
    s.replace("<!---->", "")
}

fn q(el: &Element, sel: &str) -> HtmlElement {
    el.query_selector(sel)
        .unwrap()
        .unwrap_or_else(|| panic!("no {sel}"))
        .unchecked_into()
}

fn click(el: &Element, sel: &str) {
    q(el, sel).click();
}

fn input(el: &Element, sel: &str, value: &str) {
    let i: HtmlInputElement = q(el, sel).unchecked_into();
    i.set_value(value);
    let init = web_sys::EventInit::new();
    init.set_bubbles(true);
    let e = web_sys::Event::new_with_event_init_dict("input", &init).unwrap();
    i.dispatch_event(&e).unwrap();
}

fn mount<C: BaseComponent>(props: C::Properties) -> Element {
    let el = container();
    leafy::Renderer::<C>::with_root_and_props(el.clone(), props).render();
    el
}

type TestResult = Result<(), String>;

macro_rules! eq {
    ($a:expr, $b:expr) => {{
        let a = $a;
        let b = $b;
        if a != b {
            return Err(format!("line {}: {:?} != {:?}", line!(), a, b));
        }
    }};
}

thread_local! {
    static RENDERS: Cell<u32> = const { Cell::new(0) };
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn log(s: impl Into<String>) {
    LOG.with_borrow_mut(|l| l.push(s.into()));
}

fn take_log() -> Vec<String> {
    LOG.take()
}

mod issues;
mod regressions;
mod review;

// ------------------------------------------------------------------ tests

#[function_component(Static)]
fn static_() -> Html {
    html! {
        <div class="a" id="x"><span>{"hello"}</span>{" & "}<b>{"<world>"}</b></div>
    }
}

async fn test_static() -> TestResult {
    let el = mount::<Static>(());
    tick().await;
    eq!(
        html(&el),
        r#"<div class="a" id="x"><span>hello</span> &amp; <b>&lt;world&gt;</b></div>"#
    );
    Ok(())
}

#[function_component(Counter)]
fn counter() -> Html {
    let n = use_state(|| 0);
    let onclick = {
        let n = n.clone();
        Callback::from(move |_: MouseEvent| n.set(*n + 1))
    };
    RENDERS.set(RENDERS.get() + 1);
    html! {
        <div>
            <button {onclick}>{"+"}</button>
            <span class={classes!("n", (*n % 2 == 1).then_some("odd"))} title={format!("t{}", *n)}>{*n}</span>
            if *n >= 2 {
                <i>{"two or more"}</i>
            } else if *n == 1 {
                <b>{"one"}</b>
            }
            {"end"}
        </div>
    }
}

async fn test_counter() -> TestResult {
    let el = mount::<Counter>(());
    tick().await;
    eq!(
        html(&el),
        r#"<div><button>+</button><span class="n" title="t0">0</span>end</div>"#
    );
    click(&el, "button");
    tick().await;
    eq!(
        html(&el),
        r#"<div><button>+</button><span class="n odd" title="t1">1</span><b>one</b>end</div>"#
    );
    click(&el, "button");
    tick().await;
    eq!(
        html(&el),
        r#"<div><button>+</button><span class="n" title="t2">2</span><i>two or more</i>end</div>"#
    );
    Ok(())
}

#[derive(Properties, PartialEq)]
struct ListProps {
    items: Vec<u32>,
    #[prop_or(true)]
    keyed: bool,
}

#[function_component(List)]
fn list(props: &ListProps) -> Html {
    html! {
        <ul>
            if props.keyed {
                {for props.items.iter().map(|i| html! { <li key={*i}>{*i}</li> })}
            } else {
                for i in props.items.iter() {
                    <li>{*i}</li>
                }
            }
        </ul>
    }
}

#[derive(Properties, PartialEq)]
struct ListHostProps {
    keyed: bool,
}

#[function_component(ListHost)]
fn list_host(props: &ListHostProps) -> Html {
    let items = use_state(|| vec![1u32, 2, 3]);
    let setters = [
        vec![1u32, 2, 3],
        vec![3, 2, 1],
        vec![0, 3, 2, 1, 4],
        vec![2, 4],
        vec![],
        vec![5, 6],
    ];
    let buttons = setters
        .into_iter()
        .enumerate()
        .map(|(n, v)| {
            let items = items.clone();
            let onclick = Callback::from(move |_: MouseEvent| items.set(v.clone()));
            html! { <button id={format!("b{n}")} {onclick}/> }
        })
        .collect::<Html>();
    html! {
        <>
            {buttons}
            <List items={(*items).clone()} keyed={props.keyed}/>
        </>
    }
}

async fn list_test(keyed: bool) -> TestResult {
    let el = mount::<ListHost>(ListHostProps { keyed });
    tick().await;
    let ul = || el.query_selector("ul").unwrap().unwrap();
    let lis = || {
        let ul = ul();
        let mut v = Vec::new();
        let c = ul.children();
        for i in 0..c.length() {
            v.push(c.item(i).unwrap());
        }
        v
    };
    eq!(ul().outer_html(), "<ul><li>1</li><li>2</li><li>3</li></ul>");
    let before = lis();
    click(&el, "#b1");
    tick().await;
    eq!(ul().outer_html(), "<ul><li>3</li><li>2</li><li>1</li></ul>");
    let after = lis();
    if keyed {
        // Nodes moved, not recreated.
        eq!(after[0] == before[2], true);
        eq!(after[2] == before[0], true);
    }
    click(&el, "#b2");
    tick().await;
    eq!(
        ul().outer_html(),
        "<ul><li>0</li><li>3</li><li>2</li><li>1</li><li>4</li></ul>"
    );
    click(&el, "#b3");
    tick().await;
    eq!(ul().outer_html(), "<ul><li>2</li><li>4</li></ul>");
    click(&el, "#b4");
    tick().await;
    eq!(ul().outer_html(), "<ul></ul>");
    click(&el, "#b5");
    tick().await;
    eq!(ul().outer_html(), "<ul><li>5</li><li>6</li></ul>");
    click(&el, "#b0");
    tick().await;
    eq!(ul().outer_html(), "<ul><li>1</li><li>2</li><li>3</li></ul>");
    Ok(())
}

async fn test_keyed_list() -> TestResult {
    list_test(true).await
}

async fn test_unkeyed_list() -> TestResult {
    list_test(false).await
}

#[derive(Properties, PartialEq)]
struct ChildProps {
    value: u32,
    #[prop_or_default]
    children: Html,
}

#[function_component(Child)]
fn child(props: &ChildProps) -> Html {
    log(format!("child render {}", props.value));
    html! { <p>{props.value}{props.children.clone()}</p> }
}

#[function_component(Parent)]
fn parent() -> Html {
    let a = use_state(|| 0u32);
    let b = use_state(|| 0u32);
    let inc_a = {
        let a = a.clone();
        Callback::from(move |_: MouseEvent| a.set(*a + 1))
    };
    let inc_b = {
        let b = b.clone();
        Callback::from(move |_: MouseEvent| b.set(*b + 1))
    };
    html! {
        <div>
            <button id="a" onclick={inc_a}/>
            <button id="b" onclick={inc_b}/>
            <Child value={*a}/>
            <Child value={100}><em>{*b}</em></Child>
        </div>
    }
}

async fn test_props() -> TestResult {
    let el = mount::<Parent>(());
    tick().await;
    eq!(take_log(), vec!["child render 0", "child render 100"]);
    click(&el, "#a");
    tick().await;
    // The second child's props (including children) are equal.
    eq!(take_log(), vec!["child render 1"]);
    eq!(
        html(&el),
        "<div><button id=\"a\"></button><button id=\"b\"></button><p>1</p><p>100<em>0</em></p></div>"
    );
    click(&el, "#b");
    tick().await;
    eq!(take_log(), vec!["child render 100"]);
    eq!(
        html(&el),
        "<div><button id=\"a\"></button><button id=\"b\"></button><p>1</p><p>100<em>1</em></p></div>"
    );
    Ok(())
}

#[derive(Clone, PartialEq, Debug)]
struct Theme(u32);

#[function_component(Consumer)]
fn consumer() -> Html {
    let t = use_context::<Theme>();
    log(format!("consumer {:?}", t));
    html! { <span>{t.map(|t| t.0)}</span> }
}

#[function_component(NotConsumer)]
fn not_consumer() -> Html {
    log("not consumer");
    html! { <Consumer/> }
}

#[function_component(Provider)]
fn provider() -> Html {
    let t = use_state(|| 1u32);
    let other = use_state(|| 0u32);
    let onclick = {
        let t = t.clone();
        Callback::from(move |_: MouseEvent| t.set(*t + 1))
    };
    let same = {
        let other = other.clone();
        Callback::from(move |_: MouseEvent| other.set(*other + 1))
    };
    html! {
        <>
            <button id="t" {onclick}/>
            <button id="o" onclick={same}/>
            <Consumer/>
            <ContextProvider<Theme> context={Theme(*t)}>
                <NotConsumer/>
                <ContextProvider<Theme> context={Theme(*t * 10)}>
                    <Consumer/>
                </ContextProvider<Theme>>
            </ContextProvider<Theme>>
        </>
    }
}

async fn test_context() -> TestResult {
    let el = mount::<Provider>(());
    tick().await;
    eq!(
        take_log(),
        vec![
            "consumer None",
            "not consumer",
            "consumer Some(Theme(1))",
            "consumer Some(Theme(10))"
        ]
    );
    click(&el, "#t");
    tick().await;
    // NotConsumer's props didn't change; only consumers re-render.
    let mut log = take_log();
    log.sort();
    eq!(
        log,
        vec!["consumer Some(Theme(2))", "consumer Some(Theme(20))"]
    );
    eq!(
        html(&el),
        "<button id=\"t\"></button><button id=\"o\"></button><span></span><span>2</span><span>20</span>"
    );
    click(&el, "#o");
    tick().await;
    eq!(take_log(), Vec::<String>::new());
    Ok(())
}

#[function_component(EffectChild)]
fn effect_child(props: &ChildProps) -> Html {
    let v = props.value;
    use_effect_with(v, move |v| {
        log(format!("effect {v}"));
        let v = *v;
        move || log(format!("cleanup {v}"))
    });
    use_effect(|| log("every"));
    html! { <i>{v}</i> }
}

#[function_component(EffectHost)]
fn effect_host() -> Html {
    let n = use_state(|| 0u32);
    let show = use_state(|| true);
    let inc = {
        let n = n.clone();
        Callback::from(move |_: MouseEvent| n.set(*n + 1))
    };
    let same = {
        let n = n.clone();
        Callback::from(move |_: MouseEvent| n.set(*n))
    };
    let toggle = {
        let show = show.clone();
        Callback::from(move |_: MouseEvent| show.set(!*show))
    };
    html! {
        <div>
            <button id="inc" onclick={inc}/>
            <button id="same" onclick={same}/>
            <button id="toggle" onclick={toggle}/>
            if *show {
                <EffectChild value={*n}/>
            }
        </div>
    }
}

async fn test_effects() -> TestResult {
    let el = mount::<EffectHost>(());
    tick().await;
    eq!(take_log(), vec!["effect 0", "every"]);
    click(&el, "#inc");
    tick().await;
    eq!(take_log(), vec!["cleanup 0", "effect 1", "every"]);
    click(&el, "#same");
    tick().await;
    // Re-rendered the host (use_state, not _eq), but the child's props are
    // equal, so it didn't render.
    eq!(take_log(), Vec::<String>::new());
    click(&el, "#toggle");
    tick().await;
    eq!(take_log(), vec!["cleanup 1"]);
    eq!(
        html(&el),
        "<div><button id=\"inc\"></button><button id=\"same\"></button><button id=\"toggle\"></button></div>"
    );
    click(&el, "#toggle");
    tick().await;
    eq!(take_log(), vec!["effect 1", "every"]);
    Ok(())
}

#[function_component(InputTest)]
fn input_test() -> Html {
    let value = use_state(String::new);
    let checked = use_state(|| false);
    let input_ref = use_node_ref();
    let oninput = {
        let value = value.clone();
        Callback::from(move |e: InputEvent| {
            let v = e.target_unchecked_into::<HtmlInputElement>().value();
            // Controlled: upper-case it.
            value.set(v.to_uppercase());
        })
    };
    let clear = {
        let value = value.clone();
        let checked = checked.clone();
        Callback::from(move |_: MouseEvent| {
            value.set(String::new());
            checked.set(!*checked);
        })
    };
    let r = input_ref.clone();
    use_effect_with((), move |_| {
        log(format!("ref {}", r.cast::<HtmlInputElement>().is_some()));
    });
    html! {
        <div>
            <input id="i" ref={input_ref} value={(*value).clone()} {oninput}/>
            <input id="c" type="checkbox" checked={*checked}/>
            <button id="clear" onclick={clear} disabled={value.is_empty()}/>
            <span>{(*value).clone()}</span>
        </div>
    }
}

async fn test_input() -> TestResult {
    let el = mount::<InputTest>(());
    tick().await;
    eq!(take_log(), vec!["ref true"]);
    let i = || q(&el, "#i").unchecked_into::<HtmlInputElement>();
    let c = || q(&el, "#c").unchecked_into::<HtmlInputElement>();
    eq!(q(&el, "#clear").has_attribute("disabled"), true);
    input(&el, "#i", "abc");
    tick().await;
    eq!(i().value(), "ABC");
    eq!(q(&el, "span").inner_text(), "ABC");
    eq!(q(&el, "#clear").has_attribute("disabled"), false);
    click(&el, "#clear");
    tick().await;
    eq!(i().value(), "");
    eq!(c().checked(), true);
    // The user unchecks it; a re-render with the same value fixes it.
    c().set_checked(false);
    input(&el, "#i", "x");
    tick().await;
    eq!(c().checked(), true);
    Ok(())
}

enum Msg {
    Inc,
    Add(u32),
}

#[derive(Properties, PartialEq)]
struct StructProps {
    step: u32,
}

struct StructComp {
    n: u32,
}

impl Component for StructComp {
    type Message = Msg;
    type Properties = StructProps;

    fn create(_ctx: &Context<Self>) -> Self {
        Self { n: 0 }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Msg) -> bool {
        match msg {
            Msg::Inc => self.n += ctx.props().step,
            Msg::Add(n) => {
                if n == 0 {
                    return false;
                }
                self.n += n
            }
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        log(format!("view {}", self.n));
        html! {
            <div>
                <button id="inc" onclick={ctx.link().callback(|_: MouseEvent| Msg::Inc)}/>
                <button id="add" onclick={ctx.link().callback(|_: MouseEvent| Msg::Add(10))}/>
                <button id="zero" onclick={ctx.link().callback(|_: MouseEvent| Msg::Add(0))}/>
                {self.n}
            </div>
        }
    }

    fn rendered(&mut self, _ctx: &Context<Self>, first: bool) {
        log(format!("rendered {first}"));
    }
}

async fn test_struct() -> TestResult {
    let el = mount::<StructComp>(StructProps { step: 2 });
    tick().await;
    eq!(take_log(), vec!["view 0", "rendered true"]);
    click(&el, "#inc");
    tick().await;
    eq!(take_log(), vec!["view 2", "rendered false"]);
    click(&el, "#add");
    click(&el, "#inc");
    tick().await;
    // Batched.
    eq!(take_log(), vec!["view 14", "rendered false"]);
    click(&el, "#zero");
    tick().await;
    eq!(take_log(), Vec::<String>::new());
    eq!(html(&el).ends_with("14</div>"), true);
    Ok(())
}

// (Split on purpose: the warning that it is, allowed.)
#[allow(deprecated)]
#[function_component(Splits)]
fn splits() -> Html {
    let n = use_state(|| 1u32);
    let onclick = {
        let n = n.clone();
        Callback::from(move |_: MouseEvent| n.set(*n + 1))
    };
    html! {
        <div>
            <button {onclick}/>
            <table>
                <tr><th>{"k"}</th><th>{*n}</th></tr>
                {for (0..*n).map(|i| html!{ <tr key={i}><td>{i}</td></tr> })}
            </table>
            <p>{"a"}<div>{"b"}{*n}</div>{"c"}</p>
            <select><option value="x">{"X"}</option></select>
        </div>
    }
}

async fn test_splits() -> TestResult {
    let el = mount::<Splits>(());
    tick().await;
    eq!(
        html(&el),
        "<div><button></button><table><tr><th>k</th><th>1</th></tr><tr><td>0</td></tr></table><p>a<div>b1</div>c</p><select><option value=\"x\">X</option></select></div>"
    );
    click(&el, "button");
    tick().await;
    eq!(
        html(&el),
        "<div><button></button><table><tr><th>k</th><th>2</th></tr><tr><td>0</td></tr><tr><td>1</td></tr></table><p>a<div>b2</div>c</p><select><option value=\"x\">X</option></select></div>"
    );
    Ok(())
}

/// A component whose root changes between nothing, text, a fragment and an
/// element, inside a list of keyed components that get reordered (state must
/// follow the key).
#[derive(Properties, PartialEq)]
struct ShapeProps {
    id: u32,
}

#[function_component(Shape)]
fn shape(props: &ShapeProps) -> Html {
    let s = use_state(|| 0u32);
    let onclick = {
        let s = s.clone();
        Callback::from(move |_: MouseEvent| s.set((*s + 1) % 4))
    };
    let id = props.id;
    html! {
        <>
            <button id={format!("s{id}")} {onclick}/>
            {match *s {
                0 => Html::default(),
                1 => html!{ {format!("t{id}")} },
                2 => html!{ <><b>{id}</b>{"x"}<i/></> },
                _ => html!{ <u>{id}</u> },
            }}
        </>
    }
}

#[function_component(Shapes)]
fn shapes() -> Html {
    let order = use_state(|| vec![1u32, 2, 3]);
    let rev = {
        let order = order.clone();
        Callback::from(move |_: MouseEvent| {
            let mut v = (*order).clone();
            v.reverse();
            order.set(v);
        })
    };
    html! {
        <div>
            <button id="rev" onclick={rev}/>
            {for order.iter().map(|&id| html! { <Shape key={id} {id}/> })}
            {"end"}
        </div>
    }
}

async fn test_shapes() -> TestResult {
    let el = mount::<Shapes>(());
    tick().await;
    let b = |n: u32| format!("<button id=\"s{n}\"></button>");
    let pre = "<div><button id=\"rev\"></button>";
    eq!(html(&el), format!("{pre}{}{}{}end</div>", b(1), b(2), b(3)));
    click(&el, "#s2");
    tick().await;
    eq!(
        html(&el),
        format!("{pre}{}{}t2{}end</div>", b(1), b(2), b(3))
    );
    click(&el, "#s2");
    click(&el, "#s3");
    tick().await;
    eq!(
        html(&el),
        format!("{pre}{}{}<b>2</b>x<i></i>{}t3end</div>", b(1), b(2), b(3))
    );
    click(&el, "#rev");
    tick().await;
    eq!(
        html(&el),
        format!("{pre}{}t3{}<b>2</b>x<i></i>{}end</div>", b(3), b(2), b(1))
    );
    click(&el, "#s2");
    click(&el, "#s1");
    tick().await;
    eq!(
        html(&el),
        format!("{pre}{}t3{}<u>2</u>{}t1end</div>", b(3), b(2), b(1))
    );
    click(&el, "#s2");
    click(&el, "#rev");
    tick().await;
    eq!(
        html(&el),
        format!("{pre}{}t1{}{}t3end</div>", b(1), b(2), b(3))
    );
    Ok(())
}

#[derive(Default)]
struct Counter2(u32);

impl Reducible for Counter2 {
    type Action = u32;
    fn reduce(self: Rc<Self>, action: u32) -> Rc<Self> {
        Rc::new(Counter2(self.0 + action))
    }
}

#[function_component(MiscHooks)]
fn misc_hooks() -> Html {
    let r = use_reducer(Counter2::default);
    let renders = use_mut_ref(|| 0u32);
    *renders.borrow_mut() += 1;
    let eq = use_state_eq(|| 5u32);
    let fu = use_force_update();
    let memo = use_memo(r.0 / 10, |d| {
        log(format!("memo {d}"));
        d * 2
    });
    let add = {
        let r = r.dispatcher();
        Callback::from(move |_: MouseEvent| r.dispatch(3))
    };
    let set_same = {
        let eq = eq.clone();
        Callback::from(move |_: MouseEvent| eq.set(5))
    };
    let force = Callback::from(move |_: MouseEvent| fu.force_update());
    html! {
        <div>
            <button id="add" onclick={add}/>
            <button id="same" onclick={set_same}/>
            <button id="force" onclick={force}/>
            <span>{format!("{} {} {} {}", r.0, *renders.borrow(), *eq, *memo)}</span>
        </div>
    }
}

async fn test_misc_hooks() -> TestResult {
    let el = mount::<MiscHooks>(());
    tick().await;
    let span = || q(&el, "span").inner_text();
    eq!(span(), "0 1 5 0");
    eq!(take_log(), vec!["memo 0"]);
    click(&el, "#add");
    tick().await;
    eq!(span(), "3 2 5 0");
    click(&el, "#same");
    tick().await;
    eq!(span(), "3 2 5 0");
    click(&el, "#force");
    tick().await;
    eq!(span(), "3 3 5 0");
    for _ in 0..3 {
        click(&el, "#add");
    }
    tick().await;
    eq!(span(), "12 4 5 2");
    eq!(take_log(), vec!["memo 1"]);
    Ok(())
}

#[derive(Properties, PartialEq)]
struct ModeProps {
    mode: u32,
    tag: &'static str,
}

#[function_component(Morph)]
fn morph(props: &ModeProps) -> Html {
    let t = props.tag;
    match props.mode % 4 {
        0 => Html::default(),
        1 => html! { {t} },
        2 => html! { <><i>{t}</i>{"!"}</> },
        _ => html! { <b>{t}</b> },
    }
}

/// Renders `Morph` at its root, so it has no position of its own.
#[function_component(Wrap)]
fn wrap(props: &ModeProps) -> Html {
    html! { <Morph mode={props.mode} tag={props.tag}/> }
}

#[function_component(Anchorless)]
fn anchorless() -> Html {
    let n = use_state(|| 0u32);
    let onclick = {
        let n = n.clone();
        Callback::from(move |_: MouseEvent| n.set(*n + 1))
    };
    let n = *n;
    html! {
        <>
            <button {onclick}/>
            <Wrap mode={n} tag="a"/>
            <p>{"x"}<Morph mode={n + 1} tag="b"/>{"y"}<Wrap mode={n + 2} tag="c"/></p>
            <Morph mode={n + 3} tag="d"/>
        </>
    }
}

async fn test_anchorless() -> TestResult {
    let el = mount::<Anchorless>(());
    tick().await;
    let render = |n: u32| {
        let m = |mode: u32, t: &str| match mode % 4 {
            0 => String::new(),
            1 => t.to_owned(),
            2 => format!("<i>{t}</i>!"),
            _ => format!("<b>{t}</b>"),
        };
        format!(
            "<button></button>{}<p>x{}y{}</p>{}",
            m(n, "a"),
            m(n + 1, "b"),
            m(n + 2, "c"),
            m(n + 3, "d")
        )
    };
    for n in 0..9 {
        eq!(html(&el), render(n));
        click(&el, "button");
        tick().await;
    }
    Ok(())
}

#[derive(Clone, PartialEq)]
struct Pair(u32, u32);

#[function_component(SelectFirst)]
fn select_first() -> Html {
    let first = use_context_selector(|p: &Pair| p.0).unwrap();
    log(format!("first {first}"));
    html! { <i>{first}</i> }
}

enum SMsg {
    A,
    B,
}

struct SelectorHost {
    pair: Pair,
    last_a: RefCell<Option<Callback<MouseEvent>>>,
}

impl Component for SelectorHost {
    type Message = SMsg;
    type Properties = ();

    fn create(_: &Context<Self>) -> Self {
        Self {
            pair: Pair(0, 0),
            last_a: RefCell::new(None),
        }
    }

    fn update(&mut self, _: &Context<Self>, msg: SMsg) -> bool {
        match msg {
            SMsg::A => self.pair.0 += 1,
            SMsg::B => self.pair.1 += 1,
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let a = ctx.link().callback(|_: MouseEvent| SMsg::A);
        let b = ctx.link().callback(|_: MouseEvent| SMsg::B);
        let _ = self.last_a.replace(Some(a.clone()));
        html! {
            <ContextProvider<Pair> context={self.pair.clone()}>
                <button id="a" onclick={a}/>
                <button id="b" onclick={b}/>
                <SelectFirst/>
            </ContextProvider<Pair>>
        }
    }
}

async fn test_selector() -> TestResult {
    let el = mount::<SelectorHost>(());
    tick().await;
    eq!(take_log(), vec!["first 0"]);
    click(&el, "#b");
    tick().await;
    eq!(take_log(), Vec::<String>::new());
    click(&el, "#a");
    tick().await;
    eq!(take_log(), vec!["first 1"]);
    eq!(
        html(&el),
        "<button id=\"a\"></button><button id=\"b\"></button><i>1</i>"
    );
    Ok(())
}

// ------------------------------------------------------------------- main

async fn run() {
    let tests: Vec<(
        &str,
        std::pin::Pin<Box<dyn std::future::Future<Output = TestResult>>>,
    )> = vec![
        ("static", Box::pin(test_static())),
        ("counter", Box::pin(test_counter())),
        ("keyed_list", Box::pin(test_keyed_list())),
        ("unkeyed_list", Box::pin(test_unkeyed_list())),
        ("props", Box::pin(test_props())),
        ("context", Box::pin(test_context())),
        ("effects", Box::pin(test_effects())),
        ("input", Box::pin(test_input())),
        ("struct", Box::pin(test_struct())),
        ("splits", Box::pin(test_splits())),
        ("shapes", Box::pin(test_shapes())),
        ("misc_hooks", Box::pin(test_misc_hooks())),
        ("anchorless", Box::pin(test_anchorless())),
        ("selector", Box::pin(test_selector())),
        ("svg", Box::pin(issues::test_svg())),
        ("node_ref", Box::pin(issues::test_node_ref())),
        ("children", Box::pin(issues::test_children())),
        ("effect_order", Box::pin(issues::test_effect_order())),
        ("keyed_moves", Box::pin(issues::test_keyed_moves())),
        ("keyed_fragments", Box::pin(issues::test_keyed_fragments())),
        ("attrs", Box::pin(issues::test_attrs())),
        ("struct_context", Box::pin(issues::test_struct_context())),
        ("parsing", Box::pin(issues::test_parsing())),
        ("css", Box::pin(issues::test_css())),
        ("css_same_rules", Box::pin(issues::test_css_same_rules())),
        ("vref_shape", Box::pin(review::test_vref_shape())),
        ("vref_list", Box::pin(review::test_vref_list())),
        ("blur_removed", Box::pin(review::test_blur_removed())),
        ("dead_effect", Box::pin(review::test_dead_effect())),
        ("stale_effect", Box::pin(review::test_stale_effect())),
        ("raw_svg", Box::pin(review::test_raw_svg())),
        ("parser", Box::pin(review::test_parser())),
        ("range_value", Box::pin(review::test_range_value())),
        ("svg_case", Box::pin(review::test_svg_case())),
        ("ambiguous_svg", Box::pin(review::test_ambiguous_svg())),
        ("nested_key", Box::pin(review::test_nested_key())),
        ("pending", Box::pin(review::test_pending())),
        ("reentrant", Box::pin(regressions::test_reentrant())),
        (
            "duplicate_keys",
            Box::pin(regressions::test_duplicate_keys()),
        ),
        (
            "render_sets_state",
            Box::pin(regressions::test_render_sets_state()),
        ),
        ("propagation", Box::pin(regressions::test_propagation())),
        ("foreign", Box::pin(regressions::test_foreign())),
        ("twice", Box::pin(regressions::test_twice())),
        ("uncontrolled", Box::pin(regressions::test_uncontrolled())),
        ("switching", Box::pin(regressions::test_switching())),
    ];
    let mut failed = 0;
    let total = tests.len();
    for (name, t) in tests {
        take_log();
        match t.await {
            Ok(()) => web_sys::console::log_1(&format!("PASS {name}").into()),
            Err(e) => {
                failed += 1;
                web_sys::console::log_1(&format!("FAIL {name}: {e}").into());
            }
        }
    }
    document().set_title(&format!("done: {} passed, {failed} failed", total - failed));
}

fn main() {
    leafy::set_custom_panic_hook(Box::new(|info| {
        web_sys::console::log_1(&format!("PANIC {info}").into());
        document().set_title("panicked");
    }));
    wasm_bindgen_futures::spawn_local(run());
}
