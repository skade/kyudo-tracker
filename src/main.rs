#[macro_use]
extern crate stdweb;

#[macro_use]
extern crate serde_derive;
extern crate serde_json;

extern crate futures;

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

use stdweb::{Null, Promise, PromiseFuture};

// Shamelessly stolen from webplatform's TodoMVC example.
macro_rules! enclose {
    ( ($( $x:ident ),*) $y:expr ) => {
        {
            $(let $x = $x.clone();)*
            $y
        }
    };
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

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
struct State {
    past: Vec<Session>,
    current: Session
}

js_serializable!( State );

impl State {
    fn debug(&self) {
        let debug_string = format!("{:?}", self);

        js! { 
            let state = @{debug_string};
            console.log("Current State is: " + state)
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

fn save_state( state: &StateRef ) {
    console!( log, format!( "Saving!" ) );

    let state_borrow = state.borrow();

    let db = js! {
        return new PouchDB("kyudo-track");
    };

    let state = &*state_borrow;
    let doc: PromiseFuture<String> = js! {
        var db = @{&db};
        var state =  @{state};
        console.log( state );
        state._id = "mydoc";
        return db.post(state);
    }.try_into().unwrap();
    let future = doc.and_then(|_| {
        console!( log, format!( "Saved ") );
        Ok(())
    }).or_else(|e| {
        console!( log, format!( "Hit Error: {}", e ) );
        Err(())
    });

    PromiseFuture::spawn(future);

    //
    //window().local_storage().insert( "state", state_json.as_str() ).unwrap();
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

    let db = js! {
        return new PouchDB("kyudo-track");
    };

    let doc: PromiseFuture<String> = js! {
        let db = @{&db};
        return db.get("mydoc");
    }.try_into().unwrap();

    let state_future = doc.and_then(|state_json| {

        serde_json::from_str( state_json.as_str() ).map_err(|_| stdweb::web::error::Error::new("oops"))

    }).then(|result| {
        match result {
            Ok(parsed_state) => Ok(StateRef::new(parsed_state)),
            _ => Ok(StateRef::new(State::default()))
        }        
    }).and_then(|state| {

        let register_hits_button: Element = document().query_selector( ".register-set" ).unwrap().unwrap();
        register_hits_button.add_event_listener( enclose!( (state) move |_: ClickEvent| {
            save_current_set(&state);

            save_state(&state);
            update_dom(&state);
        }));

        window().add_event_listener( enclose!( (state) move |_: HashChangeEvent| {
            save_state( &state );
            update_dom(&state);
        }));

        Ok(())
    }).or_else(|_: stdweb::web::error::Error| {
        console!( log, format!( "Hit Error loading" ) );
        Err(())
    });

    PromiseFuture::spawn(state_future);

    // let state = window().local_storage().get( "state" ).and_then( |state_json| {
    //     serde_json::from_str( state_json.as_str() ).ok()
    // }).unwrap_or_default();
    //let state = StateRef::new(state);

    // save_state( &state );
    // update_dom(&state);

    stdweb::event_loop();
}