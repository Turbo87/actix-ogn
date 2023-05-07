use std::time::Duration;

use actix::io::{FramedWrite, WriteHandler};
use actix::prelude::*;
use actix_service::Service;
use actix_tls::connect::Connector;
use backoff::backoff::Backoff;
use backoff::ExponentialBackoff;
use log::{debug, error, info, trace, warn};
use tokio::io::WriteHalf;
use tokio::net::TcpStream;
use tokio_util::codec::{FramedRead, LinesCodec, LinesCodecError};

/// Received a position record from the OGN client.
#[derive(Message, Clone)]
#[rtype(result = "()")]
pub struct OGNMessage {
    pub raw: String,
}

/// An actor that connects to the [OGN](https://www.glidernet.org/) APRS servers
pub struct OGNActor {
    recipient: Recipient<OGNMessage>,
    backoff: ExponentialBackoff,
    writer: Option<FramedWrite<String, WriteHalf<TcpStream>, LinesCodec>>,
}

impl OGNActor {
    pub fn new(recipient: Recipient<OGNMessage>) -> OGNActor {
        let mut backoff = ExponentialBackoff::default();
        backoff.max_elapsed_time = None;

        OGNActor {
            recipient,
            backoff,
            writer: None,
        }
    }

    /// Schedule sending a "keep alive" message to the server every 30sec
    fn schedule_keepalive(ctx: &mut Context<Self>) {
        ctx.run_later(Duration::from_secs(30), |act, ctx| {
            if let Some(ref mut writer) = act.writer {
                debug!("Sending keepalive to OGN server");
                writer.write("# keep alive".to_string());
            } else {
                warn!("Cannot send keepalive to OGN server, writer not set");
            }
            OGNActor::schedule_keepalive(ctx);
        });
    }
}

impl Actor for OGNActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("Connecting to OGN server...");

        Connector::default()
            .service()
            .call("aprs.glidernet.org:10152".into())
            .into_actor(self)
            .map(|res, act, ctx| match res {
                Ok(connection) => {
                    info!("Connected to OGN server");

                    // reset exponential backoff algorithm
                    act.backoff.reset();

                    let (stream, _) = connection.into_parts();
                    let (r, w) = tokio::io::split(stream);

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
                            username, password, app_name, app_version,
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

impl WriteHandler<LinesCodecError> for OGNActor {
    fn error(&mut self, err: LinesCodecError, _: &mut Self::Context) -> Running {
        warn!("OGN write error: {}", err);
        Running::Stop
    }

    fn finished(&mut self, ctx: &mut Self::Context) {
        info!("OGN write stream ended");
        ctx.stop() // reconnect later when restarted via supervisor
    }

}

/// Send received lines to the `recipient`
impl StreamHandler<Result<String, LinesCodecError>> for OGNActor {
    fn handle(&mut self, line: Result<String, LinesCodecError>, _: &mut Self::Context) {
        match line {
            Ok(line) => {
                if !line.starts_with('#') {
                    trace!("Line: {}", line);
                    if let Err(error) = self.recipient.try_send(OGNMessage { raw: line }) {
                        warn!("try_send failed: {}", error);
                    }
                } else if let Some(message) = line.strip_prefix("# ") {
                    // # <control message>
                    debug!("Control: {}", message)
                } else {
                    // #<control message>
                    debug!("Control: {}", &line[1..])
                }
            }
            Err(err) => {
                error!("OGN receive error: {}", err);
            }
        }
    }

    fn finished(&mut self, ctx: &mut Self::Context) {
        info!("OGN read stream ended");
        ctx.stop() // reconnect later when restarted via supervisor
    }
}
