#![recursion_limit="128"]

#[macro_use]
extern crate stdweb;

extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

extern crate futures;

use stdweb::JsSerialize;
use stdweb::unstable::TryFrom;
use futures::Future;

use stdweb::web::Node;
use stdweb::web::NodeList;
use std::cell::RefCell;
use std::rc::Rc;

use stdweb::traits::*;
use stdweb::unstable::TryInto;
use stdweb::web::{
    HtmlElement,
    Element,
    document,
    window
};

use stdweb::web::event::{
    DoubleClickEvent,
    ClickEvent,
    KeyPressEvent,
    ChangeEvent,
    BlurEvent,
    HashChangeEvent
};

use stdweb::web::html_element::InputElement;

use stdweb::{Value, Null};
use stdweb::{Promise, PromiseFuture};

// Shamelessly stolen from webplatform's TodoMVC example.
macro_rules! enclose {
    ( ($( $x:ident ),*) $y:expr ) => {
        {
            $(let $x = $x.clone();)*
            $y
        }
    };
}

struct Database {
    db: Value
}

impl Database {
    fn new(name: &str) -> Database {
        let db = js! {
            return new PouchDB(@{name});
        };

        Database { db: db }
    }

    fn get<T: 'static>(&self, id: &str) -> PromiseFuture<T>
    where T: TryFrom<stdweb::Value>,
          <T as stdweb::unstable::TryFrom<stdweb::Value>>::Error: std::fmt::Debug {

        let promise = js! {
            let db = @{&self.db};
            let id = @{id};

            return db.get(id);
        }.try_into().unwrap();

        promise
    }

    fn insert_or_update<T>(&self, doc: &T, id: &str) -> PromiseFuture<Value>
        where T: JsSerialize + 'static,
              T: TryFrom<stdweb::Value>,
              <T as stdweb::unstable::TryFrom<stdweb::Value>>::Error: std::fmt::Debug {

        let doc: PromiseFuture<Value> = js! {
            var db = @{&self.db};
            var new_doc = @{&doc};
            var id = @{id};

            new_doc._id = id;

            return db.get(id).then(function(doc) {
                new_doc._rev = doc._rev;
                return db.put(new_doc);
            }).catch(function(err) {
                console.log("saving new state " + new_doc);
                return db.post(new_doc);
            });
        }.try_into().unwrap();

        doc
    }

    fn bulk<I: Iterator<Item=Value>>(&self, bulk: I) {
        console!( log, "Bulk");
        for v in bulk {
            console!( log, v )
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
enum Shot {
    Hit,
    Miss,
    Shitsu
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Set {
    hits: Vec<Shot>,
}

impl Set {
    fn hits<'a>(&'a self) -> Box<Iterator<Item=&Shot> + 'a>  {
        Box::new(self.hits.iter().filter(|h| **h == Shot::Hit ))
    }

    fn misses<'a>(&'a self) -> Box<Iterator<Item=&Shot> + 'a>  {
        Box::new(self.hits.iter().filter(|h| **h == Shot::Miss ))
    }

    fn number_of_shots(&self) -> u64 {
        self.hits.len() as u64
    }

    fn had_shitsu(&self) -> bool {
        self.hits.iter().any(|h| *h == Shot::Shitsu )
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct Session {
    sets: Vec<Set>,
    _id: Option<String>,
    _rev: Option<String>,
}

impl Session {
    fn hits<'a>(&'a self) -> Box<Iterator<Item=&Shot> + 'a> {
        Box::new(self.sets.iter().flat_map(|s| s.hits() ))
    }

    fn misses<'a>(&'a self) -> Box<Iterator<Item=&Shot> + 'a> {
        Box::new(self.sets.iter().flat_map(|s| s.misses() ))
    }

    fn shots<'a>(&'a self) -> Box<Iterator<Item=&Shot> + 'a> {
        Box::new(self.sets.iter().flat_map(|s| s.hits.iter()))
    }
}

js_serializable!( Session );
js_deserializable!( Session );


#[derive(Default, Debug, Serialize, Deserialize, Clone)]
struct State {
    past: Vec<Session>,
    current: Session
}

js_serializable!( State );
js_deserializable!( State );

impl State {
    fn debug(&self) {
        let debug_string = format!("{:?}", self);

        js! { 
            let state = @{debug_string};
            console.log("Current State is: " + state)
        }
    }

    fn iter<'a>(&'a self) -> StateIterator<'a, Box<Iterator<Item=&'a Session> + 'a>> {
        let iter =  Box::new(std::iter::once(&self.current).
            chain(self.past.iter()));

        StateIterator { past_iterator: iter }
    }
}

struct StateIterator<'a, I> where I: Iterator<Item=&'a Session> + 'a {
    past_iterator: I
}

impl<'a, I> Iterator for StateIterator<'a, I>
    where I: Iterator<Item=&'a Session> {
    type Item = Value;

    fn next(&mut self) -> Option<Value> {
        if let Some(next) = self.past_iterator.next() {
            Some(next.try_into().unwrap())
        } else {
            None
        }
    }
}

#[derive(Clone)]
struct StateRef(Rc<RefCell<State>>);

impl StateRef {
    fn new(s: State) -> StateRef {
        StateRef(Rc::new(RefCell::new(s)))
    }
}

impl std::ops::Deref for StateRef {
    type Target = Rc<RefCell<State>>;

    fn deref(&self) -> &Rc<RefCell<State>> {
        &self.0
    }
}

fn save_state( state: &StateRef, db: Rc<Database> ) {

    let state_borrow = state.borrow();

    let iter = state_borrow.iter();

    db.bulk(iter);

    let insertion = db.insert_or_update(&*state_borrow, "mydoc");

    let future = insertion.and_then(|v| {
        console!( log, format!( "Saved: {:?}", v) );
        Ok(())
    }).or_else(|e| {
        console!( log, format!( "Hit Error: {}", e ) );
        Err(())
    });

    PromiseFuture::spawn(future);
}

fn save_current_set( state: &StateRef ) {
    let mato: NodeList = document().query_selector_all(".mato").unwrap();

    let shots = mato.iter().map(|m| {
        let checked: bool = js!( return @{&m}.checked; ).try_into().unwrap();
        if checked {
            Shot::Hit
        } else {
            Shot::Miss
        }
    }).collect::<Vec<_>>();

    let hits = Set { hits: shots };

    let mut state = state.borrow_mut();
    state.current.sets.push(hits);

    mato.iter().for_each(|m| {
        js!( @{&m}.checked = false; );
    });

    state.debug();
}

fn render_set_item(item: &Set, list: &Element) {
    let li: HtmlElement = document().create_element( "li" ).unwrap().try_into().unwrap();

    for shot in item.hits.iter() {
        let span: HtmlElement = document().create_element("span").unwrap().try_into().unwrap();

        let text_content = match *shot {
            Shot::Hit    => "O",
            Shot::Miss   => "X",
            Shot::Shitsu => "/",
        };
        
        let text = document().create_text_node( &text_content );
        span.append_child(&text);
        li.append_child(&span);
    }

    list.append_child(&li);
}

fn update_dom(state: &StateRef) {
    let list = document().query_selector( ".sets" ).unwrap().unwrap();

    while let Some( child ) = list.first_child() {
        list.remove_child( &child ).unwrap();
    }

    let state_borrow = state.borrow();

    for (_, set) in state_borrow.current.sets.iter().enumerate() {
        render_set_item(&set, &list)
    }

    let total = state_borrow.current.shots().count();
    let hits = state_borrow.current.hits().count();
    let misses = state_borrow.current.misses().count();
    let hit_rate = hits as f64 / total as f64;

    update_span(".number-total", &total.to_string());
    update_span(".number-of-hits", &hits.to_string());
    update_span(".number-of-misses", &misses.to_string());
    update_span(".hit-rate", &hit_rate.to_string());
}

fn update_span(selector: &str, value: &str) {
    let slot = document().query_selector(selector).unwrap().unwrap();

    while let Some( child ) = slot.first_child() {
        slot.remove_child( &child ).unwrap();
    }

    let count = format!("{}", value);
    let text = document().create_text_node( &count);

    slot.append_child(&text);
}

fn main() {
    stdweb::initialize();

    let db = Rc::new(Database::new("kyudo-track"));

    let state_future = db.get::<State>("mydoc").then(|result| {
        match result {
            Ok(parsed_state) => Ok(StateRef::new(parsed_state)),
            _ => Ok(StateRef::new(State::default()))
        }        
    }).and_then(move |state| {

        let register_hits_button: Element = document().query_selector( ".register-set" ).unwrap().unwrap();
        register_hits_button.add_event_listener( enclose!( (state, db) move |_: ClickEvent| {
            save_current_set(&state);

            save_state(&state, db.clone());
            update_dom(&state);
        }));

        window().add_event_listener( enclose!( (state, db) move |_: HashChangeEvent| {
            save_state(&state, db.clone());
            update_dom(&state);
        }));

        update_dom(&state);
        Ok(())
    }).or_else(|_: stdweb::web::error::Error| {
        console!( log, format!( "Hit Error loading" ) );
        Err(())
    });

    PromiseFuture::spawn(state_future);

    stdweb::event_loop();
}