extern crate actix;
extern crate actix_ogn;
extern crate pretty_env_logger;
extern crate aprs_parser;

use actix::*;
use actix_ogn::{OGNActor, OGNMessage};

use aprs_parser::{APRSData, Timestamp};

pub struct ConsoleLogger;

impl Actor for ConsoleLogger {
    type Context = Context<Self>;
}

impl Handler<OGNMessage> for ConsoleLogger {
    type Result = ();

    fn handle(&mut self, message: OGNMessage, _: &mut Context<Self>) {
        match message.message.data {
            APRSData::Position(position) => {
                let time = position.timestamp.map(|t| match t {
                    Timestamp::HHMMSS(h, m, s) => format!("{:02}:{:02}:{:02}", h, m, s),
                    _ => "        ".to_owned(),
                }).unwrap_or("        ".to_owned());

                println!(
                    "{}  {:9}  {:>9.4}  {:>8.4}  {}",
                    time,
                    message.message.from.call,
                    position.longitude,
                    position.latitude,
                    position.comment,
                );
            },
            _ => {},
        }
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
