extern crate actix;
extern crate actix_ogn;

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
    let sys = actix::System::new("test");

    // Start "console logger" actor in separate thread
    let logger: Addr<Syn, _> = Arbiter::start(|_| ConsoleLogger);

    // Start OGN client in separate thread
    let l = logger.clone();
    Arbiter::start(|_| OGNActor::new(l.recipient()));

    sys.run();
}
