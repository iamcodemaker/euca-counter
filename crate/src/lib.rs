use wasm_bindgen::prelude::*;
use cfg_if::cfg_if;
use log::{trace, debug, info, warn, error};

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

impl<'a, Message> Dom<Message> {
    fn dom(&'a mut self) -> Box<Iterator<Item = euca::DomItem<'a, Message>> + 'a> where
        Message: Clone,
    {
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
                .flat_map(|c| c.dom())
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

fn update(model: &mut Model, msg: Msg) {
    match msg {
        Msg::Increment => model.0 += 1,
        Msg::Decrement => model.0 -= 1,
    }
}

fn render(model: &Model) -> Vec<Dom<Msg>> {
    vec![
        button("+", Msg::Increment),
        counter(model.0),
        button("-", Msg::Decrement),
    ]
}

use std::rc::Rc;
use std::cell::RefCell;

struct App {
    dom: Vec<Dom<Msg>>,
    parent: web_sys::Element,
    model: Model,
    callback: Rc<Fn(Msg)>,
}

impl App {
    fn attach(parent: web_sys::Element, model: Model) {
        // render our initial model
        let dom = render(&model);

        // we use a RefCell here because we need the dispatch callback to be able to mutate our
        // App. This should be safe because the browser should only ever dispatch events from a
        // single thread.
        let app: Rc<RefCell<_>> = Rc::new(RefCell::new(App {
            dom: dom,
            parent: parent.clone(),
            model: model,
            callback: Rc::new(|_|()),
        }));

        let callback_app = app.clone();
        let callback = Rc::new(move |msg| {
            debug!("dispatching msg: {:?}", msg);
            callback_app.borrow_mut().dispatch(msg);
        });

        app.borrow_mut().callback = callback.clone();
        // at this point, Rc(app) references Rc(callback) which references Rc(app) creating a
        // circular reference. This will never be freed. That's fine though we need this to stick
        // around.

        // render the initial app
        use std::iter;
        debug!("rendering initial dom");

        let mut app = app.borrow_mut();

        let n = app.dom.iter_mut().flat_map(|d| d.dom());
        let patch_set = euca::diff(iter::empty(), n);
        euca::patch(parent, patch_set, callback);
    }

    fn dispatch(&mut self, msg: Msg) {
        // update the model
        update(&mut self.model, msg);

        // render a new dom from the updated model
        let mut dom = render(&self.model);

        // push changes to the browser
        debug!("rendering update");
        let old = self.dom.iter_mut().flat_map(|d| d.dom());
        let new = dom.iter_mut().flat_map(|d| d.dom());
        let patch_set = euca::diff(old, new);
        euca::patch(self.parent.clone(), patch_set, self.callback.clone());

        // store the new dom, drop the old one
        self.dom = dom;
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
        update(&mut model, Msg::Increment);
        assert_eq!(model.0, 1);
    }

    #[test]
    fn decrement() {
        let mut model = Model::new();
        update(&mut model, Msg::Decrement);
        assert_eq!(model.0, -1);
    }

    // we can also test the view/renering code by sending it a model and checking the dom that
    // comes out. This requires a custom PartialEq implementation and a custom Debug implementation
    // that ignores web_sys nodes and closures as those don't have PartialEq or Debug.
}
