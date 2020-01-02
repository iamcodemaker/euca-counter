#![recursion_limit = "128"]

use wasm_bindgen::prelude::*;
use euca::app::*;
use euca::dom::*;
use euca::typed_html::*;
use typed_html::dom::DOMTree;
use typed_html::{html, text};
use std::fmt;

struct Model(i32);

impl Model {
    fn new() -> Self {
        Model(0)
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Msg {
    Increment,
    Decrement,
}

// required by typed-html, but unused
impl fmt::Display for Msg {
    fn fmt(&self, _f: &mut fmt::Formatter) -> fmt::Result {
        unimplemented!()
    }
}

// necessary for typed-html to convert from our raw mesage type to the Handler<Msg> type the dom
// actually uses
impl From<Msg> for euca::dom::Handler<Msg> {
    fn from(msg: Msg) -> Self {
        euca::dom::Handler::Msg(msg)
    }
}

#[derive(Debug, PartialEq)]
enum Cmd { }

impl SideEffect<Msg> for Cmd {
    fn process(self, _: &Dispatcher<Msg, Self>) { }
}

impl Update<Msg, Cmd> for Model {
    fn update(&mut self, msg: Msg, _: &mut Commands<Cmd>) {
        match msg {
            Msg::Increment => self.0 += 1,
            Msg::Decrement => self.0 -= 1,
        }
    }
}

impl<'a> Render<Dom<Msg, Cmd>> for Model {
    fn render(&self) -> Dom<Msg, Cmd> {
        let tree: DOMTree<Euca<Msg>> = html!(
            <div>
                <button onclick=Msg::Increment>"+"</button>
                <div>{ text!("{}", {self.0}) }</div>
                <button onclick=Msg::Decrement>"-"</button>
            </div>
        : Euca<Msg>);
        tree.into()
    }
}

#[wasm_bindgen]
pub fn run() -> Result<(), JsValue> {
    let parent = web_sys::window()
        .expect_throw("couldn't get window handle")
        .document()
        .expect_throw("couldn't get document handle")
        .query_selector("main")
        .expect_throw("error querying for element")
        .expect_throw("expected <main></main>");

    AppBuilder::default()
        .attach(parent, Model::new());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn button(text: &str, msg: Msg) -> Dom<Msg, Cmd> {
        Dom::elem("button")
            .event("click", msg)
            .push(Dom::text(text))
    }

    fn counter(count: i32) -> Dom<Msg, Cmd> {
        Dom::elem("div")
            .push(Dom::text(count.to_string()))
    }

    // we can test the model in isolation by initializing it, then sending it the messages we want,
    // and checking that it's state is as expected. This can be done by checking individual
    // elements in the model or if the model implements PartialEq, we can check the whole model at
    // once.
    #[test]
    fn increment() {
        let mut model = Model::new();
        model.update(Msg::Increment, &mut Commands::default());
        assert_eq!(model.0, 1);
    }

    #[test]
    fn decrement() {
        let mut model = Model::new();
        model.update(Msg::Decrement, &mut Commands::default());
        assert_eq!(model.0, -1);
    }

    // we can also test the view/renering code by sending it a model and checking the dom that
    // comes out. This requires a custom PartialEq implementation and a custom Debug implementation
    // that ignores web_sys nodes and closures as those don't have PartialEq or Debug. DomItem has
    // PartialEq and Debug implementations that meet this criteria, so we can implement comparisons
    // for testing purposes in terms of the dom iterator.
    #[test]
    fn basic_render() {
        let model = Model::new();
        let dom = model.render();

        let reference: Dom<Msg, Cmd> = Dom::elem("div")
            .extend(vec![
                button("+", Msg::Increment),
                counter(0),
                button("-", Msg::Decrement),
            ])
        ;

        // here we could do this
        //
        // ```rust
        // assert!(dom.dom_iter().eq(reference.dom_iter()));
        // ```
        //
        // but we want to use assert_eq!() so we can see the contents of the dom if it doesn't
        // match

        use euca::vdom::{DomIter, DomItem};
        let dom: Vec<DomItem<Msg, Cmd>> = dom.dom_iter().collect();
        let reference: Vec<DomItem<Msg, Cmd>> = reference.dom_iter().collect();
        assert_eq!(dom, reference);
    }

    // we can also use this technique to test individual dom generation components instead of
    // testing the entire render function if necessary
}
