use wasm_bindgen::prelude::*;
use cfg_if::cfg_if;
use log::{trace, debug, info, warn, error};
use euca::{Update, Render, DomIter};
use std::fmt;

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

impl<Message> euca::DomIter<Message> for Dom<Message> where
    Message: Clone + PartialEq,
{
    fn dom_iter<'a>(&'a mut self) -> Box<Iterator<Item = euca::DomItem<'a, Message>> + 'a> {
        use std::iter;
        let iter = iter::once(&mut self.node)
            .map(|node| match node {
                Node::Text { text, ref mut node } => {
                    euca::DomItem::Text {
                        text: text,
                        node: match node {
                            Some(_) => euca::Storage::Read(Box::new(move || node.take().unwrap())),
                            None => euca::Storage::Write(Box::new(move |n| *node = Some(n))),
                        }
                    }
                }
                Node::Element { name, ref mut node } => {
                    euca::DomItem::Element {
                        element: name,
                        node: match node {
                            Some(_) => euca::Storage::Read(Box::new(move || node.take().unwrap())),
                            None => euca::Storage::Write(Box::new(move |n| *node = Some(n))),
                        }
                    }
                }
            })
            .chain(self.event.iter_mut()
                 .map(|Event { trigger, message, closure }| euca::DomItem::Event {
                     trigger: trigger,
                     handler: euca::EventHandler::Msg(message),
                     closure: match closure {
                         Some(_) => euca::Storage::Read(Box::new(move || closure.take().unwrap())),
                         None => euca::Storage::Write(Box::new(move |c| *closure = Some(c))),
                     },
                 })
            )
            .chain(self.children.iter_mut()
                .flat_map(|c| c.dom_iter())
            )
            .chain(iter::once(euca::DomItem::Up));

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

impl euca::Update<Msg> for Model {
    fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Increment => self.0 += 1,
            Msg::Decrement => self.0 -= 1,
        }
    }
}

impl euca::Render<DomVec<Msg>> for Model {
    fn render(&self) -> DomVec<Msg> {
        vec![
            button("+", Msg::Increment),
            counter(self.0),
            button("-", Msg::Decrement),
        ].into()
    }
}

struct DomVec<Msg>(Vec<Dom<Msg>>);

impl<Message> euca::DomIter<Message> for DomVec<Message> where
    Message: Clone + PartialEq,
{
    fn dom_iter<'a>(&'a mut self) -> Box<Iterator<Item = euca::DomItem<'a, Message>> + 'a> {
        Box::new(self.0.iter_mut().flat_map(|i| i.dom_iter()))
    }
}

impl<Msg> From<Vec<Dom<Msg>>> for DomVec<Msg> {
    fn from(v: Vec<Dom<Msg>>) -> Self {
        DomVec(v)
    }
}

use std::rc::Rc;
use std::cell::RefCell;

struct App<Model, DomTree> {
    dom: DomTree,
    parent: web_sys::Element,
    model: Model,
}

impl<Message, Model, DomTree> euca::Dispatch<Message> for App<Model, DomTree> where
    Message: fmt::Debug + Clone + PartialEq + 'static,
    Model: euca::Update<Message> + euca::Render<DomTree> + 'static,
    DomTree: euca::DomIter<Message> + 'static,
{
    fn dispatch(app_rc: Rc<RefCell<Self>>, msg: Message) {
        debug!("dispatching msg: {:?}", msg);

        let mut app = app_rc.borrow_mut();
        let parent = app.parent.clone();

        // update the model
        app.model.update(msg);

        // render a new dom from the updated model
        let mut new_dom = app.model.render();

        // push changes to the browser
        debug!("rendering update");
        let old = app.dom.dom_iter();
        let new = new_dom.dom_iter();
        let patch_set = euca::diff(old, new);
        euca::patch(parent, patch_set, app_rc.clone());

        app.dom = new_dom;
    }
}

fn attach<Model, Message, DomTree>(parent: web_sys::Element, model: Model) where
    Model: euca::Update<Message> + euca::Render<DomTree> + 'static,
    DomTree: euca::DomIter<Message> + 'static,
    Message: fmt::Debug + Clone + PartialEq + 'static,
{
    // render our initial model
    let dom = model.render();

    // we use a RefCell here because we need the dispatch callback to be able to mutate our
    // App. This should be safe because the browser should only ever dispatch events from a
    // single thread.
    let app_rc: Rc<RefCell<_>> = Rc::new(RefCell::new(App {
        dom: dom,
        parent: parent.clone(),
        model: model,
    }));

    // render the initial app
    use std::iter;
    debug!("rendering initial dom");

    let mut app = app_rc.borrow_mut();

    let n = app.dom.dom_iter();
    let patch_set = euca::diff(iter::empty(), n);
    euca::patch(parent, patch_set, app_rc.clone());
}

impl<Model, DomTree> App<Model, DomTree> {
    fn detach<Message>(app_rc: Rc<RefCell<App<Model, DomTree>>>) where
        Model: euca::Update<Message> + euca::Render<DomTree> + 'static,
        DomTree: euca::DomIter<Message> + 'static,
        Message: fmt::Debug + Clone + PartialEq + 'static,
    {
        // render the initial app
        use std::iter;
        debug!("rendering initial dom");

        let mut app = app_rc.borrow_mut();
        let parent = app.parent.clone();

        let o = app.dom.dom_iter();
        let patch_set = euca::diff(o, iter::empty());
        euca::patch(parent, patch_set, app_rc.clone());
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

    attach(parent, Model::new());

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
    // that ignores web_sys nodes and closures as those don't have PartialEq or Debug.
    // euca::DomItem has PartialEq and Debug implementations that meet this criteria, so we can
    // implement comparisons for testing purposes in terms of the dom iterator.
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

        let dom: Vec<euca::DomItem<Msg>> = dom.dom_iter().collect();
        let reference: Vec<euca::DomItem<Msg>> = reference.dom_iter().collect();
        assert_eq!(dom, reference);
    }

    // we can also use this technique to test individual dom generation components instead of
    // testing the entire render function if necessary
}
