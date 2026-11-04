#!/usr/bin/env python3
"""Generates src/generated.rs: N structurally distinct synthetic components
in the style of a typical yew app's UI (props with defaults, state, context,
callbacks, conditionals, lists, inputs), written against yew's API, so the
same source compiles against yew and leafy.

Usage: gen.py N > src/generated.rs
"""
import sys

N = int(sys.argv[1])

KINDS = 6

import random

def extra(i):
    """Structurally unique extra markup for component i, so wasm-opt can't
    merge components with each other (real components all differ)."""
    r = random.Random(i)
    out = []
    for k in range(r.randint(0, 4)):
        tag = r.choice(["div", "span", "p", "b", "i", "em", "small", "label"])
        attrs = ""
        for a, name in enumerate(r.sample(["title", "id", "data-x", "role", "aria-label", "tabindex"], r.randint(0, 3))):
            if r.random() < 0.5:
                attrs += f' {name}="v{i}_{k}_{a}"'
            else:
                attrs += f' {name}={{format!("d{{}}", {r.randint(0, 99)} + {k})}}'
        if r.random() < 0.3:
            attrs += f' onclick={{Callback::from(|_: MouseEvent| web_sys::console::log_1(&"{i}_{k}".into()))}}'
        body = r.choice([f'{{"text {i} {k}"}}', '', f'{{({r.randint(0, 9)}u32 + {k})}}', f'<br/>'])
        if r.random() < 0.3:
            out.append(f'if {r.randint(0, 9)} > {k} {{ <{tag}{attrs}>{body}</{tag}> }}')
        else:
            out.append(f'<{tag}{attrs}>{body}</{tag}>')
    return "\n            ".join(out)

def kind0(i):
    # Plain display component: props with defaults, classes!, optional callback.
    return f'''
#[derive(PartialEq, Properties)]
pub struct C{i}Props {{
    pub label: AttrValue,
    #[prop_or_default]
    pub onclick: Option<Callback<MouseEvent>>,
    #[prop_or(false)]
    pub selected: bool,
    #[prop_or_default]
    pub count: u32,
}}

#[function_component(C{i})]
pub fn c{i}(props: &C{i}Props) -> Html {{
    let ctx = use_context::<Ctx>().unwrap();
    let mut inline = String::new();
    if props.selected {{
        inline.push_str("cursor: default; color: #{i % 999:03};");
    }}
    html! {{
        <div
            class={{classes!("card-{i}", props.selected.then(|| "selected-{i}"))}}
            style={{inline}}
            onclick={{props.onclick.clone()}}
        >
            {extra(i)}
            <span class="label-{i}">{{props.label.clone()}}</span>
            if props.count > 0 {{
                <span class="count-{i}">{{props.count}}{{" items"}}</span>
            }}
            <div class="footer-{i}">{{ctx.name.clone()}}{{" / frame "}}{{ctx.frame}}</div>
        </div>
    }}
}}
'''

def kind1(i):
    # Stateful toggle with child component usage.
    child = f"C{i - 1}" if i % KINDS != 0 and i > 0 else None
    return f'''
#[derive(PartialEq, Properties)]
pub struct C{i}Props {{
    #[prop_or_default]
    pub title: Option<AttrValue>,
    #[prop_or_default]
    pub children: Html,
}}

#[function_component(C{i})]
pub fn c{i}(props: &C{i}Props) -> Html {{
    let open = use_state(|| false);
    let hovered = use_state_eq(|| 0u32);
    let toggle = {{
        let open = open.clone();
        Callback::from(move |_: MouseEvent| open.set(!*open))
    }};
    let onmouseenter = {{
        let hovered = hovered.clone();
        Callback::from(move |_: MouseEvent| hovered.set(*hovered + 1))
    }};
    html! {{
        <section class="panel-{i}" {{onmouseenter}}>
            {extra(i)}
            <h2 class="panel-title-{i}" onclick={{toggle}}>
                {{props.title.clone().unwrap_or_else(|| AttrValue::from("Panel {i}"))}}
                <small>{{" hovered "}}{{*hovered}}</small>
            </h2>
            if *open {{
                <div class="panel-body-{i}">
                    {{props.children.clone()}}
                    <p>{{"This is panel number {i}, which is open."}}</p>
                </div>
            }} else {{
                <p class="closed-{i}">{{"Closed"}}</p>
            }}
        </section>
    }}
}}
'''

def kind2(i):
    # List rendering from context, keyed.
    return f'''
#[derive(PartialEq, Properties)]
pub struct C{i}Props {{
    #[prop_or(10)]
    pub limit: usize,
    #[prop_or_default]
    pub onselect: Callback<usize>,
}}

#[function_component(C{i})]
pub fn c{i}(props: &C{i}Props) -> Html {{
    let ctx = use_context::<Ctx>().unwrap();
    let items = ctx.items.iter().take(props.limit).enumerate().map(|(n, item)| {{
        let onclick = props.onselect.reform(move |_: MouseEvent| n);
        html! {{
            <li key={{item.id}} class={{classes!("row-{i}", (n % 2 == 0).then_some("even"))}} {{onclick}}>
                <span class="name-{i}">{{item.name.clone()}}</span>
                <span class="score-{i}">{{item.score}}</span>
            </li>
        }}
    }}).collect::<Html>();
    html! {{
        <ul class="list-{i}">
            {{items}}
            <li>{extra(i)}</li>
            if ctx.items.is_empty() {{
                <li class="empty-{i}">{{"Nothing here ({i})"}}</li>
            }}
        </ul>
    }}
}}
'''

def kind3(i):
    # Form input with node ref and effect.
    return f'''
#[derive(PartialEq, Properties)]
pub struct C{i}Props {{
    #[prop_or_default]
    pub placeholder: Option<AttrValue>,
    pub onsubmit: Callback<String>,
}}

#[function_component(C{i})]
pub fn c{i}(props: &C{i}Props) -> Html {{
    let value = use_state(String::new);
    let input_ref = use_node_ref();
    {{
        let input_ref = input_ref.clone();
        use_effect_with((), move |_| {{
            if let Some(input) = input_ref.cast::<HtmlInputElement>() {{
                let _ = input.focus();
            }}
            || ()
        }});
    }}
    let oninput = {{
        let value = value.clone();
        Callback::from(move |e: InputEvent| {{
            value.set(e.target_unchecked_into::<HtmlInputElement>().value());
        }})
    }};
    let onkeydown = {{
        let value = value.clone();
        let onsubmit = props.onsubmit.clone();
        Callback::from(move |e: KeyboardEvent| {{
            if e.key() == "Enter" {{
                onsubmit.emit((*value).clone());
            }}
        }})
    }};
    html! {{
        <div class="form-{i}">
            {extra(i)}
            <label for="input-{i}">{{"Field {i}"}}</label>
            <input
                id="input-{i}"
                type="text"
                ref={{input_ref}}
                value={{(*value).clone()}}
                placeholder={{props.placeholder.clone()}}
                {{oninput}}
                {{onkeydown}}
                autocomplete="off"
            />
            <button disabled={{value.is_empty()}} onclick={{
                let onsubmit = props.onsubmit.clone();
                let value = value.clone();
                Callback::from(move |_: MouseEvent| onsubmit.emit((*value).clone()))
            }}>{{"Submit"}}</button>
        </div>
    }}
}}
'''

def kind4(i):
    # Composite: uses earlier generated components with props.
    base = i - (i % KINDS)
    return f'''
#[derive(PartialEq, Properties)]
pub struct C{i}Props {{
    #[prop_or_default]
    pub mode: u8,
}}

#[function_component(C{i})]
pub fn c{i}(props: &C{i}Props) -> Html {{
    let selected = use_state_eq(|| None::<usize>);
    let onselect = {{
        let selected = selected.clone();
        Callback::from(move |n: usize| selected.set(Some(n)))
    }};
    let onsubmit = Callback::from(|s: String| web_sys::console::log_1(&s.into()));
    html! {{
        <div class="composite-{i}">
            {extra(i)}
            <C{base} label="Composite {i}" selected={{selected.is_some()}} count={{props.mode as u32}}/>
            <C{base + 1} title={{AttrValue::from("Title {i}")}}>
                <C{base + 2} limit={{5}} {{onselect}}/>
            </C{base + 1}>
            if let Some(n) = *selected {{
                <p class="selected-{i}">{{format!("Selected {{n}} in {i}")}}</p>
            }}
            <C{base + 3} {{onsubmit}} placeholder="Type {i}"/>
        </div>
    }}
}}
'''

def kind5(i):
    # Match-heavy display with many static elements, like a dialog.
    return f'''
#[derive(Clone, Copy, PartialEq)]
pub enum E{i} {{ A, B, C }}

#[derive(PartialEq, Properties)]
pub struct C{i}Props {{
    pub which: E{i},
    #[prop_or_default]
    pub extra: Option<AttrValue>,
}}

#[function_component(C{i})]
pub fn c{i}(props: &C{i}Props) -> Html {{
    let ctx = use_context::<Ctx>().unwrap();
    let body = match props.which {{
        E{i}::A => html! {{ <p class="a-{i}">{{"Option A for dialog {i}."}}</p> }},
        E{i}::B => html! {{
            <>
                <p class="b-{i}">{{"Option B for dialog {i}."}}</p>
                <a href="https://example.com/{i}" target="_blank">{{"Learn more"}}</a>
            </>
        }},
        E{i}::C => html! {{ <p class="c-{i}"><b>{{"Option C"}}</b>{{format!(" at frame {{}}", ctx.frame)}}</p> }},
    }};
    html! {{
        <div class="dialog-{i}" style="position: absolute; top: 0; left: 0;">
            {extra(i)}
            <h1 class="dialog-title-{i}">{{"Dialog {i}"}}</h1>
            <table class="table-{i}">
                <tr><th>{{"Key"}}</th><th>{{"Value"}}</th></tr>
                <tr><td>{{"Frame"}}</td><td>{{ctx.frame}}</td></tr>
                <tr><td>{{"Name"}}</td><td>{{ctx.name.clone()}}</td></tr>
            </table>
            {{body}}
            if let Some(extra) = props.extra.clone() {{
                <div class="extra-{i}">{{extra}}</div>
            }}
        </div>
    }}
}}
'''

KIND_FNS = [kind0, kind1, kind2, kind3, kind4, kind5]

def usage(i):
    k = i % KINDS
    if k == 0:
        return f'<C{i} label="Root {i}" count={{3}}/>'
    if k == 1:
        return f'<C{i}><span>{{"child {i}"}}</span></C{i}>'
    if k == 2:
        return f'<C{i}/>'
    if k == 3:
        return f'<C{i} onsubmit={{Callback::from(|_: String| ())}}/>'
    if k == 4:
        return f'<C{i} mode={{2}}/>'
    if k == 5:
        return f'<C{i} which={{E{i}::B}}/>'

out = ['// Generated by gen.py. Do not edit.', '#![allow(unused)]',
       'use crate::Ctx;',
       'use web_sys::{HtmlInputElement, InputEvent, KeyboardEvent, MouseEvent};',
       'use yew::prelude::*;',
       'use yew::TargetCast;',
       '']
# Round N down to whole groups so composites have their siblings.
groups = N // KINDS
for i in range(groups * KINDS):
    out.append(KIND_FNS[i % KINDS](i))

out.append('#[function_component(GenRoot)]')
out.append('pub fn gen_root() -> Html {')
out.append('    html! {')
out.append('        <>')
for i in range(groups * KINDS):
    out.append('            ' + usage(i))
out.append('        </>')
out.append('    }')
out.append('}')
print('\n'.join(out))
