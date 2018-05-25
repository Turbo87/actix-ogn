#[macro_use]
extern crate log;

extern crate tokio_core;
extern crate tokio_io;

#[macro_use]
extern crate actix;

extern crate aprs_parser;
extern crate backoff;

use std::io;
use std::time::Duration;

use actix::actors::{Connect, Connector};
use actix::prelude::*;
use actix::io::{FramedWrite, WriteHandler};
use tokio_core::net::TcpStream;
use tokio_io::codec::{FramedRead, LinesCodec};
use tokio_io::io::WriteHalf;
use tokio_io::AsyncRead;

use backoff::backoff::Backoff;
use backoff::ExponentialBackoff;

/// Received a position record from the OGN client.
#[derive(Message, Clone)]
pub struct OGNMessage {
    pub message: aprs_parser::APRSMessage,
    pub raw: String,
}

/// An actor that connects to the [OGN](https://www.glidernet.org/) APRS servers
pub struct OGNActor {
    recipient: Recipient<Syn, OGNMessage>,
    backoff: ExponentialBackoff,
    cell: Option<FramedWrite<WriteHalf<TcpStream>, LinesCodec>>,
}

impl OGNActor {
    pub fn new(recipient: Recipient<Syn, OGNMessage>) -> OGNActor {
        let backoff = ExponentialBackoff::default();
        OGNActor { recipient, backoff, cell: None }
    }

    /// Schedule sending a "keep alive" message to the server every 30sec
    fn schedule_keepalive(ctx: &mut Context<Self>) {
        ctx.run_later(Duration::from_secs(30), |act, ctx| {
            info!("Sending keepalive to OGN server");
            if let Some(ref mut framed) = act.cell {
                framed.write("# keep alive".to_string());
            }
            OGNActor::schedule_keepalive(ctx);
        });
    }
}

impl Actor for OGNActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("Connecting to OGN server...");

        Connector::from_registry()
            .send(Connect::host("glidern1.glidernet.org:10152"))
            .into_actor(self)
            .map(|res, act, ctx| match res {
                Ok(stream) => {
                    info!("Connected to OGN server");

                    // reset exponential backoff algorithm
                    act.backoff.reset();

                    let (r, w) = stream.split();

                    // configure write side of the connection
                    let mut framed = FramedWrite::new(w, LinesCodec::new(), ctx);

                    // send login message
                    let login_message = {
                        let username = "test";
                        let password = "-1";
                        let app_name = option_env!("CARGO_PKG_NAME").unwrap_or("unknown");
                        let app_version = option_env!("CARGO_PKG_VERSION").unwrap_or("0.0.0");

                        format!(
                            "user {} pass {} vers {} {}",
                            username,
                            password,
                            app_name,
                            app_version,
                        )
                    };

                    framed.write(login_message);

                    // save writer for later
                    act.cell = Some(framed);

                    // read side of the connection
                    ctx.add_stream(FramedRead::new(r, LinesCodec::new()));

                    // schedule sending a "keep alive" message to the server every 30sec
                    OGNActor::schedule_keepalive(ctx);
                }
                Err(err) => {
                    error!("Can not connect to OGN server: {}", err);

                    // re-connect with exponential backoff
                    if let Some(timeout) = act.backoff.next_backoff() {
                        ctx.run_later(timeout, |_, ctx| ctx.stop());
                    } else {
                        ctx.stop();
                    }
                }
            })
            .map_err(|err, act, ctx| {
                error!("Can not connect to OGN server: {}", err);

                // re-connect with exponential backoff
                if let Some(timeout) = act.backoff.next_backoff() {
                    ctx.run_later(timeout, |_, ctx| ctx.stop());
                } else {
                    ctx.stop();
                }
            })
            .wait(ctx);
    }

    fn stopped(&mut self, _: &mut Self::Context) {
        info!("Disconnected from OGN server");
    }
}

impl Supervised for OGNActor {
    fn restarting(&mut self, _: &mut Self::Context) {
        info!("Restarting OGN client...");
        self.cell.take();
    }
}

impl WriteHandler<io::Error> for OGNActor {
    fn error(&mut self, err: io::Error, _: &mut Self::Context) -> Running {
        warn!("OGN connection dropped: error: {}", err);
        Running::Stop
    }
}

/// Parse received lines into `OGNPositionRecord` instances
/// and send them to the `recipient`
impl StreamHandler<String, io::Error> for OGNActor {
    fn handle(&mut self, raw: String, _: &mut Self::Context) {
        if raw.starts_with('#') {
            info!("{}", raw);
        } else {
            trace!("{}", raw);

            match aprs_parser::parse(&raw) {
                Ok(message) => {
                    trace!("{:?}", message);
                    self.recipient.do_send(OGNMessage { message, raw });
                },
                Err(error) => {
                    warn!("ParseError: {}", error);
                }
            };
        }
    }
}
