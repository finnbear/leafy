// Synthetic benchmark: a root that renders N generated components
// (src/generated.rs, written by gen.py), all of which read a context that
// changes each "frame" (a click of #frame, so speed.mjs can time it).

mod generated;

use std::rc::Rc;
use yew::prelude::*;

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

#[function_component(App)]
fn app() -> Html {
    let frame = use_state(|| 0u32);
    let next = {
        let frame = frame.clone();
        Callback::from(move |_: MouseEvent| frame.set(*frame + 1))
    };
    let items = use_memo((), |_| {
        (0..20)
            .map(|id| Item {
                id,
                name: AttrValue::from(format!("Item {id}")),
                score: id as i32 * 7,
            })
            .collect::<Vec<_>>()
    });
    let ctx = Ctx {
        frame: *frame,
        name: AttrValue::from("bench"),
        items: Rc::new((*items).clone()),
    };
    html! {
        <>
            <button id="frame" onclick={next}>{"next frame"}</button>
            <ContextProvider<Ctx> context={ctx}>
                <generated::GenRoot/>
            </ContextProvider<Ctx>>
        </>
    }
}

fn main() {
    yew::Renderer::<App>::new().render();
}
