extern crate actix;
extern crate actix_ogn;
extern crate pretty_env_logger;

use actix::*;
use actix_ogn::{OGNActor, OGNMessage};

pub struct ConsoleLogger;

impl Actor for ConsoleLogger {
    type Context = Context<Self>;
}

impl Handler<OGNMessage> for ConsoleLogger {
    type Result = ();

    fn handle(&mut self, message: OGNMessage, _: &mut Context<Self>) {
        println!("{}", message.raw);
    }
}

fn main() {
    pretty_env_logger::init();

    let sys = actix::System::new("test");

    // Start "console logger" actor in separate thread
    let logger: Addr<_> = ConsoleLogger.start();

    // Start OGN client in separate thread
    let _addr: Addr<_> = Supervisor::start(move |_| OGNActor::new(logger.recipient()));

    sys.run();
}
