use std::io;
use std::time::Duration;

use actix::actors::resolver::{Connect, Resolver};
use actix::prelude::*;
use actix::io::{FramedWrite, WriteHandler};
use tokio_tcp::TcpStream;
use tokio_codec::{FramedRead, LinesCodec};
use tokio_io::io::WriteHalf;
use tokio_io::AsyncRead;

use backoff::backoff::Backoff;
use backoff::ExponentialBackoff;
use log::{error, info, trace, warn};

/// Received a position record from the OGN client.
#[derive(Message, Clone)]
pub struct OGNMessage {
    pub raw: String,
}

/// An actor that connects to the [OGN](https://www.glidernet.org/) APRS servers
pub struct OGNActor {
    recipient: Recipient<OGNMessage>,
    backoff: ExponentialBackoff,
    writer: Option<FramedWrite<WriteHalf<TcpStream>, LinesCodec>>,
}

impl OGNActor {
    pub fn new(recipient: Recipient<OGNMessage>) -> OGNActor {
        let mut backoff = ExponentialBackoff::default();
        backoff.max_elapsed_time = None;

        OGNActor { recipient, backoff, writer: None }
    }

    /// Schedule sending a "keep alive" message to the server every 30sec
    fn schedule_keepalive(ctx: &mut Context<Self>) {
        ctx.run_later(Duration::from_secs(30), |act, ctx| {
            info!("Sending keepalive to OGN server");
            if let Some(ref mut writer) = act.writer {
                writer.write("# keep alive".to_string());
            }
            OGNActor::schedule_keepalive(ctx);
        });
    }
}

impl Actor for OGNActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("Connecting to OGN server...");

        Resolver::from_registry()
            .send(Connect::host("aprs.glidernet.org:10152"))
            .into_actor(self)
            .map(|res, act, ctx| match res {
                Ok(stream) => {
                    info!("Connected to OGN server");

                    // reset exponential backoff algorithm
                    act.backoff.reset();

                    let (r, w) = stream.split();

                    // configure write side of the connection
                    let mut writer = FramedWrite::new(w, LinesCodec::new(), ctx);

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

                    writer.write(login_message);

                    // save writer for later
                    act.writer = Some(writer);

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
        self.writer.take();
    }
}

impl WriteHandler<io::Error> for OGNActor {
    fn error(&mut self, err: io::Error, _: &mut Self::Context) -> Running {
        warn!("OGN connection dropped: error: {}", err);
        Running::Stop
    }
}

/// Send received lines to the `recipient`
impl StreamHandler<String, io::Error> for OGNActor {
    fn handle(&mut self, line: String, _: &mut Self::Context) {
        trace!("{}", line);

        if !line.starts_with('#') {
            if let Err(error) = self.recipient.do_send(OGNMessage { raw: line }) {
                warn!("do_send failed: {}", error);
            }
        }
    }
}
