extern crate tokio_core;
extern crate tokio_io;
extern crate regex;
#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate actix;

mod ogn_client;

pub use ogn_client::{OGNClient, OGNRecord};
