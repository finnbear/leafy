// Server-side rendering benchmark: renders each scene to a string many
// times, natively, and prints the median time per render (JSON).
//
// Written against yew's API, so the same source builds against yew and
// against leafy (under yew's name). The one difference is the renderer:
// yew's is async (it supports suspense), leafy's isn't.
//
// Usage: app [SAMPLES=30]

#[path = "generated.rs"]
mod generated;

use std::hint::black_box;
use std::rc::Rc;
use std::time::Instant;
use yew::prelude::*;

// ------------------------------------------------- 100 components
// The synthetic app's generated components (from synthetic/gen.py), which
// read this context.

#[derive(Clone, PartialEq)]
pub struct Item {
    pub id: u32,
    pub name: AttrValue,
    pub score: i32,
}

#[derive(Clone, PartialEq)]
pub struct Ctx {
    pub frame: u32,
    pub name: AttrValue,
    pub items: Rc<Vec<Item>>,
}

#[derive(Properties, PartialEq)]
struct ComponentsProps {
    ctx: Ctx,
}

#[function_component(Components)]
fn components(props: &ComponentsProps) -> Html {
    html! {
        <ContextProvider<Ctx> context={props.ctx.clone()}>
            <generated::GenRoot/>
        </ContextProvider<Ctx>>
    }
}

// ---------------------------------------------------- 1,000-row table
// js-framework-benchmark's rows.

#[derive(Properties, PartialEq)]
struct RowProps {
    id: usize,
    label: AttrValue,
    selected: bool,
}

#[function_component(Row)]
fn row(props: &RowProps) -> Html {
    html! {
        <tr class={if props.selected { "danger" } else { "" }}>
            <td class="col-md-1">{ props.id }</td>
            <td class="col-md-4"><a class="lbl">{ props.label.clone() }</a></td>
            <td class="col-md-1">
                <a class="remove"><span class="glyphicon glyphicon-remove remove" aria-hidden="true"></span></a>
            </td>
            <td class="col-md-6"></td>
        </tr>
    }
}

#[derive(Properties, PartialEq)]
struct TableProps {
    rows: Rc<Vec<(usize, AttrValue)>>,
    selected: usize,
}

#[function_component(Table)]
fn table(props: &TableProps) -> Html {
    html! {
        <table class="table table-hover table-striped test-data">
            <tbody>
                { for props.rows.iter().map(|(id, label)| html! {
                    <Row key={*id} id={*id} label={label.clone()} selected={*id == props.selected}/>
                }) }
            </tbody>
        </table>
    }
}

// --------------------------------------------------------- an article
// A long page of text: sections of headings, paragraphs with links and
// emphasis, and lists (as docs, a wiki or a blog render).

#[derive(Clone, PartialEq)]
struct Section {
    title: AttrValue,
    paragraphs: Vec<AttrValue>,
    links: Vec<(AttrValue, AttrValue)>,
}

#[derive(Properties, PartialEq)]
struct ArticleProps {
    sections: Rc<Vec<Section>>,
}

#[derive(Properties, PartialEq)]
struct SectionProps {
    section: Section,
    number: usize,
}

#[function_component(SectionView)]
fn section_view(props: &SectionProps) -> Html {
    let s = &props.section;
    html! {
        <section id={format!("s{}", props.number)}>
            <h2>{ props.number }{ ". " }{ s.title.clone() }</h2>
            { for s.paragraphs.iter().map(|p| html! { <p>{ p.clone() }{ " " }<em>{ "(see below)" }</em></p> }) }
            <ul class="links">
                { for s.links.iter().map(|(href, text)| html! { <li><a href={href.clone()}>{ text.clone() }</a></li> }) }
            </ul>
        </section>
    }
}

#[function_component(Article)]
fn article(props: &ArticleProps) -> Html {
    html! {
        <article>
            <h1>{ "An article" }</h1>
            <nav><ol>
                { for props.sections.iter().enumerate().map(|(i, s)| html! { <li><a href={format!("#s{i}")}>{ s.title.clone() }</a></li> }) }
            </ol></nav>
            { for props.sections.iter().enumerate().map(|(i, s)| html! { <SectionView section={s.clone()} number={i}/> }) }
        </article>
    }
}

// ----------------------------------------------------------- harness

#[cfg(feature = "leafy")]
fn render<C: BaseComponent>(props: C::Properties) -> String {
    yew::ServerRenderer::<C>::with_props(props).render()
}

#[cfg(not(feature = "leafy"))]
fn render<C: BaseComponent>(props: C::Properties) -> String {
    use std::future::Future;
    use std::pin::pin;
    use std::task::{Context, Poll, Waker};
    // Without suspense, it's done on the first poll.
    let mut future = pin!(yew::LocalServerRenderer::<C>::with_props(props).hydratable(false).render());
    match future.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(html) => html,
        Poll::Pending => panic!("rendering suspended"),
    }
}

/// The median time (µs) of one call of `f`, over `samples` samples of
/// enough calls to take ~20 ms, after warming up; and its output's length.
fn measure(samples: usize, f: impl Fn() -> String) -> (f64, usize) {
    let len = f().len();
    let start = Instant::now();
    let mut warm = 0;
    while start.elapsed().as_millis() < 300 {
        black_box(f());
        warm += 1;
    }
    let per_sample = (warm as f64 / 15.0).ceil().max(1.0) as usize;
    let mut times: Vec<f64> = (0..samples)
        .map(|_| {
            let t = Instant::now();
            for _ in 0..per_sample {
                black_box(f());
            }
            t.elapsed().as_secs_f64() * 1e6 / per_sample as f64
        })
        .collect();
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (times[samples / 2], len)
}

fn main() {
    let samples: usize = std::env::args().nth(1).map_or(30, |s| s.parse().unwrap());
    let words = ["pretty", "large", "big", "small", "tall", "short", "red", "blue", "table", "chair", "house", "pony"];
    let rows: Rc<Vec<(usize, AttrValue)>> = Rc::new(
        (1..=1000)
            .map(|i| (i, AttrValue::from(format!("{} {} {}", words[i % 12], words[(i / 12) % 12], words[(i / 144) % 12]))))
            .collect(),
    );
    let sections: Rc<Vec<Section>> = Rc::new(
        (0..50)
            .map(|i| Section {
                title: format!("Section {i}: {} & {}", words[i % 12], words[(i + 5) % 12]).into(),
                paragraphs: (0..4)
                    .map(|j| format!("Paragraph {j} of section {i}. Lorem ipsum dolor sit amet, <consectetur> adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua; {} {}.", words[j % 12], words[(i * j) % 12]).into())
                    .collect(),
                links: (0..3)
                    .map(|j| (format!("/docs/{i}/{j}?q={}", words[j % 12]).into(), format!("Link {j}").into()))
                    .collect(),
            })
            .collect(),
    );
    let ctx = Ctx {
        frame: 0,
        name: AttrValue::from("bench"),
        items: Rc::new((0..20).map(|id| Item { id, name: format!("Item {id}").into(), score: id as i32 * 7 }).collect()),
    };

    let scenes: Vec<(&str, (f64, usize))> = vec![
        ("table", measure(samples, || render::<Table>(TableProps { rows: rows.clone(), selected: 5 }))),
        ("components", measure(samples, || render::<Components>(ComponentsProps { ctx: ctx.clone() }))),
        ("article", measure(samples, || render::<Article>(ArticleProps { sections: sections.clone() }))),
    ];
    // With $SSR_DUMP, also writes each scene's HTML there (to compare).
    if let Ok(dir) = std::env::var("SSR_DUMP") {
        let html = [
            ("table", render::<Table>(TableProps { rows: rows.clone(), selected: 5 })),
            ("components", render::<Components>(ComponentsProps { ctx: ctx.clone() })),
            ("article", render::<Article>(ArticleProps { sections: sections.clone() })),
        ];
        for (name, html) in html {
            std::fs::write(format!("{dir}/{name}.html"), html).unwrap();
        }
    }
    let entries: Vec<String> = scenes
        .iter()
        .map(|(name, (us, len))| format!("\"{name}\": {{\"us\": {us:.2}, \"bytes\": {len}}}"))
        .collect();
    println!("{{{}}}", entries.join(", "));
}
