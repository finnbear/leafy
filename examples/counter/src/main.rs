//! A counter and a keyed list: `trunk serve` in this directory.

use leafy::prelude::*;
use web_sys::HtmlInputElement;

#[derive(Properties, PartialEq)]
struct ItemProps {
    name: AttrValue,
    on_remove: Callback<AttrValue>,
}

#[function_component(Item)]
fn item(props: &ItemProps) -> Html {
    let onclick = {
        let name = props.name.clone();
        props.on_remove.reform(move |_: MouseEvent| name.clone())
    };
    html! {
        <li>{props.name.clone()}{" "}<button {onclick}>{"×"}</button></li>
    }
}

#[function_component(App)]
fn app() -> Html {
    let count = use_state(|| 0);
    let items = use_state(Vec::<AttrValue>::new);
    let input = use_node_ref();

    let inc = {
        let count = count.clone();
        Callback::from(move |_: MouseEvent| count.set(*count + 1))
    };
    let add = {
        let (items, input) = (items.clone(), input.clone());
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let input = input.cast::<HtmlInputElement>().unwrap();
            let name = input.value();
            if !name.is_empty() && !items.contains(&AttrValue::from(name.clone())) {
                let mut v = (*items).clone();
                v.push(name.into());
                items.set(v);
            }
            input.set_value("");
        })
    };
    let on_remove = {
        let items = items.clone();
        Callback::from(move |name: AttrValue| {
            items.set(items.iter().filter(|i| **i != name).cloned().collect())
        })
    };

    html! {
        <main>
            <h1>{"Count: "}{*count}</h1>
            <button onclick={inc}>{"+1"}</button>
            if *count >= 10 {
                <p>{"That's a lot."}</p>
            }
            <form onsubmit={add}>
                <input ref={input} placeholder="Add an item"/>
            </form>
            <ul>
                for name in items.iter() {
                    <Item key={name.to_string()} name={name.clone()} on_remove={on_remove.clone()}/>
                }
            </ul>
        </main>
    }
}

fn main() {
    leafy::Renderer::<App>::new().render();
}
