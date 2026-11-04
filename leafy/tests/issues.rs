//! Native (server rendering) tests for bugs found by reviewing yew's issue
//! tracker (numbers are yew issues), and for syntax that must compile.

use leafy::prelude::*;
use leafy::virtual_dom::VChild;
use leafy::{RenderedCss, ServerRenderer};

fn render<C: BaseComponent>(props: C::Properties) -> String {
    ServerRenderer::<C>::with_props(props).render()
}

/// Renders `html` (as the output of a component).
fn render_html(html: Html) -> String {
    #[derive(Properties, PartialEq)]
    struct P {
        html: Html,
    }
    #[function_component(Show)]
    fn show(p: &P) -> Html {
        p.html.clone()
    }
    render::<Show>(P { html })
}

// #175, #2938, #3129
#[test]
fn raw_text_not_escaped() {
    let css = String::from("a > b { content: \"&\" }");
    let js = "if (1 < 2 && 3 > 2) {}";
    assert_eq!(
        render_html(html! { <><style>{css}</style><script>{js}</script><b>{"a > b"}</b></> }),
        "<style>a > b { content: \"&\" }</style><script>if (1 < 2 && 3 > 2) {}</script><b>a &gt; b</b>"
    );
}

// #3296
#[test]
fn textarea_value_is_content() {
    let v = String::from("a<b & c");
    assert_eq!(
        render_html(html! { <textarea class="t" value={v} rows="2"/> }),
        "<textarea class=\"t\" rows=\"2\">a&lt;b &amp; c</textarea>"
    );
    assert_eq!(
        render_html(html! { <textarea value={None::<String>}/> }),
        "<textarea></textarea>"
    );
}

#[derive(Properties, PartialEq)]
struct WrapProps {
    #[prop_or_default]
    children: Children,
}

#[function_component(Count)]
fn count(props: &WrapProps) -> Html {
    let n = props.children.len();
    html! {
        <ul data-n={n.to_string()}>
            for c in &props.children {
                <li>{c}</li>
            }
        </ul>
    }
}

// #1262, #3256, #4073
#[test]
fn children_are_separate() {
    let xs = ["x", "y"];
    assert_eq!(
        render_html(html! {
            <Count><b>{"A"}</b>{"B"}<b>{"C"}</b>{for xs.iter().map(|x| html! { <i>{*x}</i> })}</Count>
        }),
        "<ul data-n=\"5\"><li><b>A</b></li><li>B</li><li><b>C</b></li><li><i>x</i></li><li><i>y</i></li></ul>"
    );
    assert_eq!(
        render_html(html! { <Count><b>{"A"}</b></Count> }),
        "<ul data-n=\"1\"><li><b>A</b></li></ul>"
    );
    assert_eq!(render_html(html! { <Count/> }), "<ul data-n=\"0\"></ul>");
}

#[derive(Properties, PartialEq, Clone)]
struct ItemProps {
    v: i32,
}

#[function_component(Item)]
fn item(p: &ItemProps) -> Html {
    html! { <b>{p.v}</b> }
}

#[derive(Properties, PartialEq)]
struct ListProps {
    #[prop_or_default]
    children: ChildrenWithProps<Item>,
    #[prop_or_default]
    side: Option<VChild<Item>>,
}

#[function_component(List)]
fn list(p: &ListProps) -> Html {
    let sum: i32 = p.children.iter().map(|c| c.props.v).sum();
    html! {
        <p data-sum={sum.to_string()}>{for p.children.iter()}{p.side.clone()}</p>
    }
}

// #1527, #2267, #3157
#[test]
fn children_with_props() {
    let side: VChild<Item> = html_nested! { <Item v={10}/> };
    assert_eq!(
        render_html(html! {
            <List {side}><Item v={1}/><Item v={2}/>if false { <Item v={3}/> }</List>
        }),
        "<p data-sum=\"3\"><b>1</b><b>2</b><b>10</b></p>"
    );
}

// #36, #1444
#[test]
fn keyed_fragment() {
    assert_eq!(
        render_html(html! {
            <dl>{for [1, 2].iter().map(|i| html! { <key={*i}><dt>{i}</dt><dd/></> })}</dl>
        }),
        "<dl><dt>1</dt><dd></dd><dt>2</dt><dd></dd></dl>"
    );
}

// #2574
#[test]
fn negative_literals() {
    #[derive(Properties, PartialEq)]
    struct SliderProps {
        min: f64,
        max: i32,
    }
    #[function_component(Slider)]
    fn slider(p: &SliderProps) -> Html {
        html! { <input type="range" min={p.min.to_string()} max={p.max.to_string()}/> }
    }
    assert_eq!(
        render_html(html! { <><input min=-5 max=5/><Slider min=-100.5 max=-1/></> }),
        "<input min=\"-5\" max=\"5\"><input type=\"range\" min=\"-100.5\" max=\"-1\">"
    );
}

// #3189
#[test]
fn checked_is_boolean() {
    let on = false;
    assert_eq!(
        render_html(
            html! { <><my-el checked=false/><my-el checked={!on}/><input type="checkbox" checked={on}/></> }
        ),
        "<my-el></my-el><my-el checked=\"\"></my-el><input type=\"checkbox\">"
    );
}

// #452, #2910
#[test]
fn parsing() {
    assert_eq!(
        render_html(html! { <a href="x"><img src="y"></a> }),
        "<a href=\"x\"><img src=\"y\"></a>"
    );
    assert_eq!(render_html(html! { <p>{"x"}<br></p> }), "<p>x<br></p>");
    assert_eq!(
        render_html(html! { <p>{ /* comment */ }<i/></p> }),
        "<p><i></i></p>"
    );
    assert_eq!(
        render_html(html! { <object><param name="a"></object> }),
        "<object><param name=\"a\"></object>"
    );
}

// #1677
#[derive(Properties, PartialEq)]
struct UnitProps;

#[derive(Properties, PartialEq)]
struct EmptyProps {}

#[function_component(UnitComp)]
fn unit_comp(_: &UnitProps) -> Html {
    html! { {"unit"} }
}

#[function_component(EmptyComp)]
fn empty_comp(_: &EmptyProps) -> Html {
    html! { {"empty"} }
}

// #3557
#[derive(Properties, PartialEq)]
struct TreeProps {
    #[prop_or_default]
    sub: Option<Box<Self>>,
    v: u32,
}

#[function_component(Tree)]
fn tree(p: &TreeProps) -> Html {
    html! { <i>{p.v}{p.sub.as_ref().map(|s| s.v)}</i> }
}

// #2283
#[derive(Properties, PartialEq)]
struct GenProps<T: PartialEq> {
    v: T,
}

#[function_component(Gen)]
fn gen_comp<T: PartialEq + std::fmt::Display + 'static = u8>(p: &GenProps<T>) -> Html {
    html! { {p.v.to_string()} }
}

#[test]
fn properties_shapes() {
    assert_eq!(
        render_html(html! {
            <>
                <UnitComp/><EmptyComp/>
                <Tree v={1} sub={Box::new(TreeProps { sub: None, v: 2 })}/>
                <Gen<String> v={String::from("s")}/>
                <Gen v={3u8}/>
            </>
        }),
        "unitempty<i>12</i>s3"
    );
}

// #3996, #2280: compile, with warnings (that event names are case
// sensitive, and of the duplicate).
#[allow(deprecated)]
#[test]
fn warnings() {
    let _ = html! { <form onSubmit={Callback::from(|_: Event| ())}/> };
    assert_eq!(
        render_html(html! { <i a="1" b="2" a="3"/> }),
        "<i a=\"1\" b=\"2\" a=\"3\"></i>"
    );
}

// css! and global_css!.
#[test]
fn css_macros() {
    #[function_component(Styled)]
    fn styled() -> Html {
        global_css!("body { margin: 0 }").register();
        let a = css!(
            "color: red; &:hover { color: blue; } @media (max-width: 10px) { margin: 0 1px; }"
        );
        let b = css!(".x { font-weight: bold }");
        html! {
            <>
                <i class={a}/>
                <b class={classes!("plain", b, a)}/>
            </>
        }
    }
    let (out, rendered) = ServerRenderer::<Styled>::new().render_with_css();
    // In order of first use, each once.
    assert_eq!(rendered.len(), 3);
    assert_eq!(rendered[0].class(), "");
    assert_eq!(rendered[0].css(), "body{margin:0}");
    let (a, b) = (rendered[1].class(), rendered[2].class());
    assert!(a.starts_with("sc-") && a.len() == 13, "{a}");
    assert_ne!(a, b);
    assert_eq!(
        rendered[1].css(),
        format!(
            ".{a}{{color:red}}.{a}:hover{{color:blue}}@media (max-width:10px){{.{a}{{margin:0 1px}}}}"
        )
    );
    assert_eq!(rendered[2].css(), format!(".{b} .x{{font-weight:bold}}"));
    assert_eq!(
        out,
        format!("<i class=\"{a}\"></i><b class=\"plain {b} {a}\"></b>")
    );
    assert_eq!(
        rendered.styles(),
        format!(
            "<style>body{{margin:0}}</style><style>{}</style><style>{}</style>",
            rendered[1].css(),
            rendered[2].css()
        )
    );
    // The same rules make the same class.
    assert_eq!(
        css!("color: red; &:hover { color: blue; } @media (max-width: 10px) { margin: 0 1px; }")
            .css()
            .class(),
        a
    );
    // Without collecting, the same output.
    assert_eq!(render::<Styled>(()), out);
}

/// The classes of `css`, in order.
fn classes_of(css: &RenderedCss) -> Vec<&'static str> {
    css.iter().map(|css| css.class()).collect()
}

/// Renders `html` (as the output of a component), with its stylesheets.
fn render_html_with_css(html: Html) -> (String, RenderedCss) {
    #[derive(Properties, PartialEq)]
    struct P {
        html: Html,
    }
    #[function_component(C)]
    fn c(p: &P) -> Html {
        p.html.clone()
    }
    ServerRenderer::<C>::with_props(P { html }).render_with_css()
}

// The stylesheets go wherever their classes do: to the render that outputs
// them, wherever their `Html` or `Classes` was built.
#[test]
fn css_follows_html() {
    let a = css!("color: red;");
    let b = css!("color: blue;");
    // Built before the render (as properties), and registered nowhere else.
    let html = html! { <p class={a}><i class={classes!("x", b)}/></p> };
    let (_, css) = render_html_with_css(html.clone());
    assert_eq!(classes_of(&css), [a.css().class(), b.css().class()]);
    // Cached, and rendered again: as much the second render's.
    let (_, again) = render_html_with_css(html.clone());
    assert_eq!(classes_of(&again), classes_of(&css));
    // Classes built apart from `html!`, and cloned.
    let classes: Classes = classes!(b, "y");
    let (out, css) = render_html_with_css(html! { <div class={classes.clone()}/> });
    assert_eq!(classes_of(&css), [b.css().class()]);
    assert_eq!(out, format!("<div class=\"{} y\"></div>", b.css().class()));
    // Nothing is left over for a render without them: not what an earlier
    // render used, nor what was registered outside a render.
    global_css!("p { margin: 0 }").register();
    let _ = Classes::from(a);
    let (_, css) = render_html_with_css(html! { <div class="plain"/> });
    assert!(css.is_empty());
    // A class's stylesheets don't make it unequal (as a property).
    assert!(classes!(a) == Classes::from(a.css().class()));
}

// A render within a render (e.g. to embed its HTML): the outer render uses
// the inner's stylesheets too.
#[test]
fn css_nested_render() {
    #[function_component(Inner)]
    fn inner() -> Html {
        html! { <b class={css!("color: green;")}/> }
    }
    #[function_component(Outer)]
    fn outer() -> Html {
        let (inner, css) = ServerRenderer::<Inner>::new().render_with_css();
        assert_eq!(css.len(), 1);
        html! { <div class={css!("color: gray;")}>{Html::from_html_unchecked(inner.into())}</div> }
    }
    let (out, css) = ServerRenderer::<Outer>::new().render_with_css();
    assert_eq!(css.len(), 2);
    assert!(
        out.contains(css[0].class()) && out.contains(css[1].class()),
        "{out}"
    );
}

// A whole page: the stylesheets at the end of its <head>.
#[test]
fn css_render_page() {
    #[derive(Properties, PartialEq)]
    struct P {
        title: AttrValue,
    }
    #[function_component(Page)]
    fn page(p: &P) -> Html {
        global_css!("body { margin: 0 }").register();
        html! {
            <html>
                <head><meta name="description" content={p.title.clone()}/><title>{p.title.clone()}</title></head>
                <body><header class={css!("color: red;")}>{"Hi"}</header></body>
            </html>
        }
    }
    let page = ServerRenderer::<Page>::with_props(P {
        title: "</head> x".into(),
    })
    .render_page();
    let red = css!("color: red;").css();
    assert_eq!(
        page,
        format!(
            "<!DOCTYPE html><html><head><meta name=\"description\" content=\"</head> x\"><title>&lt;/head&gt; x</title><style>body{{margin:0}}</style><style>{}</style></head><body><header class=\"{}\">Hi</header></body></html>",
            red.css(),
            red.class()
        )
    );
    // Without a <head>: just after the doctype.
    #[function_component(Bare)]
    fn bare() -> Html {
        html! { <header class={css!("color: red;")}/> }
    }
    assert_eq!(
        ServerRenderer::<Bare>::new().render_page(),
        format!(
            "<!DOCTYPE html><style>{}</style><header class=\"{}\"></header>",
            red.css(),
            red.class()
        )
    );
}

// Code review: a hole expression returning early drops the holes before it,
// whose drop can use `html!` (was a double free).
#[test]
fn early_return_drops_holes_safely() {
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            let (a, b) = (String::from("nested-1"), String::from("nested-2"));
            let _h: Html = html! { <p class={a} id={b}></p> };
        }
    }
    fn view(fail: bool) -> Option<Html> {
        let g = Guard;
        Some(html! {
            <div
                onclick={move |_: MouseEvent| { let _ = &g; }}
                onmousedown={move |_: MouseEvent| { let _ = String::from("x"); }}
                onmouseup={ if fail { return None } else { |_: MouseEvent| () } }></div>
        })
    }
    for _ in 0..3 {
        assert!(view(true).is_none());
        assert!(view(false).is_some());
    }
}

// Code review: SSR and text-only elements.
#[test]
fn ssr_text_and_comments() {
    let name = String::from("Page");
    let html = html! {
        <>
            <title>{name}{" - site & co"}</title>
            <div title="a<!>b">{"x"}{1}</div>
            <script>{"var s = '<!>';"}</script>
            <pre>{"\n"}{"x"}</pre>
        </>
    };
    assert_eq!(
        render_html(html),
        "<title>Page - site &amp; co</title><div title=\"a<!>b\">x1</div><script>var s = '<!>';</script><pre>\n\nx</pre>"
    );
}

// Code review: const generic defaults on function components.
#[function_component(ConstGen)]
fn const_gen<const N: usize = 3>() -> Html {
    html! { {N} }
}

#[test]
fn const_generic_default() {
    assert_eq!(render_html(html! { <><ConstGen/><ConstGen<5>/></> }), "35");
}

// Server rendering can't be broken out of, crash or be left broken.
#[test]
fn ssr_script_injection_escaped() {
    let evil = String::from("1</SCRIPT><img src=x onerror=alert(1)>");
    let ok = String::from("if (a < b && c > d) {}");
    assert_eq!(
        render_html(html! { <><script>{evil}</script><script>{ok}</script></> }),
        "<script>1&lt;/SCRIPT&gt;&lt;img src=x onerror=alert(1)&gt;</script><script>if (a < b && c > d) {}</script>"
    );
    let css = String::from("a{}</style><b>");
    assert_eq!(
        render_html(html! { <style>{css}</style> }),
        "<style>a{}&lt;/style&gt;&lt;b&gt;</style>"
    );
}

#[derive(Properties, PartialEq)]
struct NestProps {
    depth: u32,
}

/// Renders itself `depth` deep (as, say, a component rendering a nested
/// document's nodes does).
#[function_component(Nest)]
fn nest(p: &NestProps) -> Html {
    if p.depth == 0 {
        html! { <b>{"leaf"}</b> }
    } else {
        html! { <i><Nest depth={p.depth - 1}/></i> }
    }
}

#[test]
fn ssr_deep_nesting_is_cut_off() {
    // Deep enough to overflow a 2 MiB stack if rendered.
    let out = std::thread::Builder::new()
        .stack_size(2 << 20)
        .spawn(|| render::<Nest>(NestProps { depth: 1_000_000 }))
        .unwrap()
        .join()
        .unwrap();
    assert!(
        out.starts_with("<i><i>") && !out.contains("leaf") && out.len() < 10_000,
        "{}",
        out.len()
    );
    assert!(render::<Nest>(NestProps { depth: 100 }).contains("leaf"));
}

#[test]
fn ssr_recovers_from_panic() {
    #[derive(Properties, PartialEq)]
    struct P {
        fail: bool,
    }
    #[function_component(MaybePanic)]
    fn maybe_panic(p: &P) -> Html {
        let s = use_state(|| 1);
        s.set(2);
        if p.fail {
            panic!("boom");
        }
        html! { {*s} }
    }
    let r = std::panic::catch_unwind(|| render::<MaybePanic>(P { fail: true }));
    assert!(r.is_err());
    assert_eq!(render::<MaybePanic>(P { fail: false }), "1");
}

// As yew: batch_callback's function may return an Option or a Vec of
// messages.
#[test]
fn batch_callback_option_or_vec() {
    struct C;
    impl Component for C {
        type Message = u32;
        type Properties = ();
        fn create(_: &Context<Self>) -> Self {
            C
        }
        fn view(&self, ctx: &Context<Self>) -> Html {
            let some = ctx.link().batch_callback(|n: u32| (n > 0).then_some(n));
            let many = ctx.link().batch_callback(|n: u32| vec![n, n]);
            let _ = (some, many);
            html! { <i/> }
        }
    }
    assert_eq!(render::<C>(()), "<i></i>");
}

// props!, as yew's: defaults, shorthand, conversion, `Comp::Properties`.
#[test]
fn props_macro() {
    #[derive(Properties, PartialEq, Debug)]
    struct P {
        id: u32,
        #[prop_or_default]
        name: AttrValue,
        #[prop_or(7)]
        n: u32,
        #[prop_or_default]
        r#type: AttrValue,
    }
    #[function_component(C)]
    fn c(p: &P) -> Html {
        html! { <b>{p.id}{p.name.clone()}{p.n}</b> }
    }
    let id = 3;
    let p = leafy::props!(P { id, name: "x" });
    assert_eq!(
        (p.id, p.name.as_str(), p.n, p.r#type.as_str()),
        (3, "x", 7, "")
    );
    let p = leafy::props!(C::Properties {
        id: 1,
        n: 2,
        r#type: "t"
    });
    assert_eq!(
        (p.id, p.name.as_str(), p.n, p.r#type.as_str()),
        (1, "", 2, "t")
    );
    assert_eq!(render::<C>(leafy::props!(P { id: 4 })), "<b>47</b>");
}

// `..base` with typed children (`ChildrenWithProps`): converted, as without
// a base (was a type error).
#[test]
fn base_props_with_typed_children() {
    #[derive(Properties, PartialEq, Clone)]
    struct ItemProps {
        n: u32,
    }
    #[function_component(Item)]
    fn item(p: &ItemProps) -> Html {
        html! { <i>{p.n}</i> }
    }
    #[derive(Properties, PartialEq, Clone)]
    struct ListProps {
        #[prop_or_default]
        title: AttrValue,
        #[prop_or_default]
        children: ChildrenWithProps<Item>,
    }
    #[function_component(List)]
    fn list(p: &ListProps) -> Html {
        html! { <ul title={p.title.clone()}>{for p.children.iter()}</ul> }
    }
    #[function_component(Page)]
    fn page() -> Html {
        let base = leafy::props!(ListProps { title: "t" });
        html! {
            <List ..base>
                {for (1..3).map(|n| html_nested! { <Item {n}/> })}
            </List>
        }
    }
    assert_eq!(render::<Page>(()), "<ul title=\"t\"><i>1</i><i>2</i></ul>");
}

// `value` and `checked` are still evaluated first (as yew evaluates them),
// though applied after the other attributes; and rendered after them.
#[test]
fn prop_evaluation_order() {
    use std::cell::RefCell;
    thread_local! {
        static LOG: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
    }
    fn log(s: &'static str) -> &'static str {
        LOG.with_borrow_mut(|l| l.push(s));
        s
    }
    let html = html! {
        <input min={log("min")} value={log("value")} class={log("class")} max={log("max")}/>
    };
    assert_eq!(LOG.take(), ["value", "class", "min", "max"]);
    assert_eq!(
        render_html(html),
        "<input class=\"class\" min=\"min\" max=\"max\" value=\"value\">"
    );
}
