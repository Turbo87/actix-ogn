extern crate actix;
extern crate actix_ogn;
extern crate pretty_env_logger;

use actix::*;
use actix_ogn::{OGNActor, OGNRecord};

pub struct ConsoleLogger;

impl Actor for ConsoleLogger {
    type Context = Context<Self>;
}

impl Handler<OGNRecord> for ConsoleLogger {
    type Result = ();

    fn handle(&mut self, record: OGNRecord, _: &mut Context<Self>) {
        println!("{}", record.record.raw);
    }
}

fn main() {
    pretty_env_logger::init();

    let sys = actix::System::new("test");

    // Start "console logger" actor in separate thread
    let logger: Addr<Syn, _> = Arbiter::start(|_| ConsoleLogger);

    // Start OGN client in separate thread
    let l = logger.clone();
    Arbiter::start(|_| OGNActor::new(l.recipient()));

    sys.run();
}
