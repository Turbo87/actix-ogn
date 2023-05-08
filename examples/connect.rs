extern crate actix;
extern crate actix_ogn;
extern crate pretty_env_logger;

use actix::*;
use actix_ogn::{OGNActor, OGNMessage};

pub struct ConsoleLogger;

impl Actor for ConsoleLogger {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        // Increase mailbox size from 16 to 128 to handle bursts of messages better.
        // Otherwise try_send of OGNActor may discard messages when the system is under load.
        ctx.set_mailbox_capacity(128);
    }
}

impl Handler<OGNMessage> for ConsoleLogger {
    type Result = ();

    fn handle(&mut self, message: OGNMessage, _: &mut Context<Self>) {
        println!("{}", message.raw);
    }
}

#[actix::main]
async fn main() {
    pretty_env_logger::init();
    let logger = ConsoleLogger.start();
    let _ogn_addr = Supervisor::start(|_| OGNActor::new(logger.recipient()));
    tokio::signal::ctrl_c().await.unwrap();
    System::current().stop();
}
