//! Server-side rendering tests (native).

use leafy::ServerRenderer;
use leafy::prelude::*;

fn render<C: BaseComponent>(props: C::Properties) -> String {
    ServerRenderer::<C>::with_props(props).render()
}

#[derive(Clone, PartialEq)]
struct Theme(&'static str);

#[derive(Properties, PartialEq)]
struct ItemProps {
    name: AttrValue,
    #[prop_or(false)]
    selected: bool,
    #[prop_or_default]
    children: Html,
}

#[function_component(Item)]
fn item(props: &ItemProps) -> Html {
    let theme = use_context::<Theme>().map(|t| t.0).unwrap_or("none");
    let state = use_state(|| 5);
    use_effect_with((), |_| -> () { panic!("effects don't run on the server") });
    html! {
        <li class={classes!("item", props.selected.then_some("sel"))} data-theme={theme} onclick={Callback::from(|_: MouseEvent| ())}>
            {props.name.clone()}{" "}{*state}
            {props.children.clone()}
        </li>
    }
}

// (Its `<tr>` directly in a `<table>` is split on purpose.)
#[allow(deprecated)]
#[function_component(Page)]
fn page() -> Html {
    let items = ["a & b", "<c>", "d\"e"];
    html! {
        <ContextProvider<Theme> context={Theme("dark")}>
            <h1 title="x \"y\"">{"Title & more"}</h1>
            <ul>
                {for items.iter().enumerate().map(|(i, &n)| html! {
                    <Item key={i} name={n} selected={i == 1}>
                        if i == 2 { <b>{"bold"}</b> }
                    </Item>
                })}
            </ul>
            <input type="checkbox" checked={true} disabled={false} value={"v<"}/>
            <p>{"a"}{1}{"b"}</p>
            {Html::from_html_unchecked(AttrValue::Static("<i>raw</i>"))}
            <table><tr><td>{"cell"}</td></tr></table>
        </ContextProvider<Theme>>
    }
}

#[test]
fn ssr_page() {
    let s = render::<Page>(());
    assert_eq!(
        s,
        concat!(
            r#"<h1 title="x &quot;y&quot;">Title &amp; more</h1>"#,
            r#"<ul>"#,
            r#"<li class="item" data-theme="dark">a &amp; b 5</li>"#,
            r#"<li class="item sel" data-theme="dark">&lt;c&gt; 5</li>"#,
            r#"<li class="item" data-theme="dark">d"e 5<b>bold</b></li>"#,
            r#"</ul>"#,
            r#"<input type="checkbox" checked="" value="v<">"#,
            r#"<p>a1b</p>"#,
            r#"<i>raw</i>"#,
            r#"<table><tr><td>cell</td></tr></table>"#,
        )
    );
}

struct Counter {
    n: u32,
}

impl Component for Counter {
    type Message = ();
    type Properties = ();

    fn create(_ctx: &Context<Self>) -> Self {
        Self { n: 7 }
    }

    fn view(&self, _ctx: &Context<Self>) -> Html {
        html! { <span>{self.n}</span> }
    }
}

#[test]
fn ssr_struct_component() {
    assert_eq!(render::<Counter>(()), "<span>7</span>");
}

#[function_component(EarlyReturn)]
fn early_return() -> Html {
    let one = 1;
    let partial = |fail: bool| -> Option<Html> {
        Some(html! { <i>{one}{ if fail { None::<u32>? } else { 2 } }{one + 2}</i> })
    };
    html! { <p>{"a"}{partial(true)}{"b"}{partial(false)}{"c"}</p> }
}

#[test]
fn ssr_early_return_in_hole() {
    assert_eq!(render::<EarlyReturn>(()), "<p>ab<i>123</i>c</p>");
}

#[derive(Properties, PartialEq)]
struct LinkProps {
    #[prop_or_default]
    children: Children,
}

#[function_component(Link)]
fn link(props: &LinkProps) -> Html {
    html! { <a href="#l">{props.children.clone()}</a> }
}

// Keys on elements other than roots, and an `<a>` component in SVG.
#[function_component(Keys)]
fn keys() -> Html {
    html! {
        <div>
            <span key="a" class="x">{1}</span>
            <><b key="b"/><i/></>
            <svg><Link><circle r="1"/></Link><title>{"t"}</title></svg>
        </div>
    }
}

#[test]
fn ssr_nested_keys_and_ambiguous() {
    assert_eq!(
        render::<Keys>(()),
        r##"<div><span class="x">1</span><b></b><i></i><svg><a href="#l"><circle r="1"></circle></a><title>t</title></svg></div>"##
    );
}
