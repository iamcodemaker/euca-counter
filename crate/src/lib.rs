use wasm_bindgen::prelude::*;
use cfg_if::cfg_if;
use log::{trace, debug, info, warn, error};
use euca::dom::*;
use euca::app::*;

cfg_if! {
    // When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
    // allocator.
    if #[cfg(feature = "wee_alloc")] {
        #[global_allocator]
        static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;
    }
}

pub struct Model(i32);

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Increment,
    Decrement,
}

impl Model {
    fn new() -> Self {
        Model(0)
    }
}

enum Node {
    Text { text: String, node: Option<web_sys::Text> },
    Element { name: String, node: Option<web_sys::Element> },
}

struct Event<Message> {
    trigger: String,
    message: Message,
    closure: Option<Closure<FnMut(web_sys::Event)>>,
}

struct Dom<Message> {
    node: Node,
    event: Option<Event<Message>>,
    children: Vec<Dom<Message>>,
}

impl<Message> DomIter<Message> for Dom<Message> where
    Message: Clone + PartialEq,
{
    fn dom_iter<'a>(&'a mut self) -> Box<Iterator<Item = DomItem<'a, Message>> + 'a> {
        use std::iter;
        let iter = iter::once(&mut self.node)
            .map(|node| match node {
                Node::Text { text, ref mut node } => {
                    DomItem::Text {
                        text: text,
                        node: match node {
                            Some(_) => Storage::Read(Box::new(move || node.take().unwrap())),
                            None => Storage::Write(Box::new(move |n| *node = Some(n))),
                        }
                    }
                }
                Node::Element { name, ref mut node } => {
                    DomItem::Element {
                        element: name,
                        node: match node {
                            Some(_) => Storage::Read(Box::new(move || node.take().unwrap())),
                            None => Storage::Write(Box::new(move |n| *node = Some(n))),
                        }
                    }
                }
            })
            .chain(self.event.iter_mut()
                 .map(|Event { trigger, message, closure }| DomItem::Event {
                     trigger: trigger,
                     handler: EventHandler::Msg(message),
                     closure: match closure {
                         Some(_) => Storage::Read(Box::new(move || closure.take().unwrap())),
                         None => Storage::Write(Box::new(move |c| *closure = Some(c))),
                     },
                 })
            )
            .chain(self.children.iter_mut()
                .flat_map(|c| c.dom_iter())
            )
            .chain(iter::once(DomItem::Up));

        Box::new(iter)
    }
}

fn button(text: &str, msg: Msg) -> Dom<Msg> {
    Dom {
        node: Node::Element {
            name: "button".to_owned(),
            node: None,
        },
        event: Some(Event {
            trigger: "click".to_owned(),
            message: msg,
            closure: None,
        }),
        children: vec![
            Dom {
                node: Node::Text {
                    text: text.to_owned(),
                    node: None,
                },
                event: None,
                children: vec![],
            },
        ],
    }
}

fn counter(count: i32) -> Dom<Msg> {
    Dom {
        node: Node::Element {
            name: "div".to_owned(),
            node: None,
        },
        event: None,
        children: vec![
            Dom {
                node: Node::Text {
                    text: count.to_string(),
                    node: None,
                },
                event: None,
                children: vec![],
            },
        ],
    }
}

impl Update<Msg> for Model {
    fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Increment => self.0 += 1,
            Msg::Decrement => self.0 -= 1,
        }
    }
}

impl Render<DomVec<Msg>> for Model {
    fn render(&self) -> DomVec<Msg> {
        vec![
            button("+", Msg::Increment),
            counter(self.0),
            button("-", Msg::Decrement),
        ].into()
    }
}

struct DomVec<Msg>(Vec<Dom<Msg>>);

impl<Message> DomIter<Message> for DomVec<Message> where
    Message: Clone + PartialEq,
{
    fn dom_iter<'a>(&'a mut self) -> Box<Iterator<Item = DomItem<'a, Message>> + 'a> {
        Box::new(self.0.iter_mut().flat_map(|i| i.dom_iter()))
    }
}

impl<Msg> From<Vec<Dom<Msg>>> for DomVec<Msg> {
    fn from(v: Vec<Dom<Msg>>) -> Self {
        DomVec(v)
    }
}

cfg_if! {
    if #[cfg(feature = "console_error_panic_hook")] {
        fn set_panic_hook() {
            console_error_panic_hook::set_once();
        }
    }
    else {
        fn set_panic_hook() {}
    }
}

cfg_if! {
    if #[cfg(feature = "console_log")] {
        fn init_log() {
            console_log::init_with_level(log::Level::Trace)
                .expect("error initializing log");
        }
    }
    else {
        fn init_log() {}
    }
}

#[wasm_bindgen]
pub fn run() -> Result<(), JsValue> {
    set_panic_hook();
    init_log();

    let parent = web_sys::window()
        .expect("couldn't get window handle")
        .document()
        .expect("couldn't get document handle")
        .query_selector("main")
        .expect("error querying for element")
        .expect("expected <main></main>");

    App::attach(parent, Model::new());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // we can test the model in isolation by initializing it, then sending it the messages we want,
    // and checking that it state is as expected. This can be done by checking individual elements
    // in the model or if the model implements PartialEq, we can check the whole model at once.
    #[test]
    fn increment() {
        let mut model = Model::new();
        model.update(Msg::Increment);
        assert_eq!(model.0, 1);
    }

    #[test]
    fn decrement() {
        let mut model = Model::new();
        model.update(Msg::Decrement);
        assert_eq!(model.0, -1);
    }

    // we can also test the view/renering code by sending it a model and checking the dom that
    // comes out. This requires a custom PartialEq implementation and a custom Debug implementation
    // that ignores web_sys nodes and closures as those don't have PartialEq or Debug.  DomItem has
    // PartialEq and Debug implementations that meet this criteria, so we can implement comparisons
    // for testing purposes in terms of the dom iterator.
    #[test]
    fn basic_render() {
        let model = Model::new();
        let mut dom = model.render();

        let mut reference: DomVec<Msg> = vec![
            button("+", Msg::Increment),
            counter(0),
            button("-", Msg::Decrement),
        ].into();

        // here we could do this
        //
        // ```rust
        // assert!(dom.dom_iter().eq(reference.dom_iter()));
        // ```
        //
        // but we want to use assert_eq!() so we can see the contents of the dom if it doesn't
        // match

        let dom: Vec<DomItem<Msg>> = dom.dom_iter().collect();
        let reference: Vec<DomItem<Msg>> = reference.dom_iter().collect();
        assert_eq!(dom, reference);
    }

    // we can also use this technique to test individual dom generation components instead of
    // testing the entire render function if necessary
}
