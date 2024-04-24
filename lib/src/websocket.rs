#![allow(clippy::result_large_err)]

// use std::future::Future;
// use std::pin::Pin;
// use tokio::net::TcpStream;
// use futures::{StreamExt, SinkExt};
// use tokio_tungstenite::MaybeTlsStream;
// use tokio_tungstenite::WebSocketStream;
// use tokio_tungstenite::connect_async;
use crate::config::Config;
use crate::errors::{DreamrunnerError, DreamrunnerResult};
use crate::model::{
    AccountUpdateEvent, BalanceUpdateEvent, KlineEvent, OrderTradeEvent, TradeEvent,
};
use log::*;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;
use url::Url;
use tokio_tungstenite::tungstenite::Message;
use std::net::TcpStream;
use tokio_tungstenite::tungstenite::handshake::client::Response;
use tokio_tungstenite::tungstenite::protocol::WebSocket;
use tokio_tungstenite::tungstenite::stream::MaybeTlsStream;
use tokio_tungstenite::tungstenite::connect;

#[allow(clippy::all)]
enum WebSocketAPI {
    Default,
    MultiStream,
    Custom(String),
}

impl WebSocketAPI {
    fn params(self, subscription: &str, testnet: bool) -> String {
        match testnet {
            true => match self {
                WebSocketAPI::Default => {
                    format!("wss://testnet.binance.vision/ws/{}", subscription)
                }
                WebSocketAPI::MultiStream => format!(
                    "wss://testnet.binance.vision/stream?streams={}",
                    subscription
                ),
                WebSocketAPI::Custom(url) => format!("{}/{}", url, subscription),
            },
            false => match self {
                WebSocketAPI::Default => {
                    format!("wss://stream.binance.us:9443/ws/{}", subscription)
                }
                WebSocketAPI::MultiStream => format!(
                    "wss://stream.binance.us:9443/stream?streams={}",
                    subscription
                ),
                WebSocketAPI::Custom(url) => format!("{}/{}", url, subscription),
            },
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum WebSocketEvent {
    AccountUpdate(AccountUpdateEvent),
    BalanceUpdate(BalanceUpdateEvent),
    OrderTrade(OrderTradeEvent),
    Trade(TradeEvent),
    Kline(KlineEvent),
}

pub type Callback<'a> = Box<dyn FnMut(WebSocketEvent) -> DreamrunnerResult<()> + 'a>;
pub type CallbackInner<'a> = dyn FnMut(WebSocketEvent) -> DreamrunnerResult<()> + 'a;
// pub type Callback = Box<dyn FnMut(WebSocketEvent) -> Pin<Box<dyn Future<Output = DreamrunnerResult<()>> + Send>> + Sync>;

pub struct WebSockets<'a> {
    pub socket: Option<(WebSocket<MaybeTlsStream<TcpStream>>, Response)>,
    // pub socket: Option<(WebSocketStream<MaybeTlsStream<TcpStream>>, Response)>,
    handler: Callback<'a>,
    testnet: bool,
    last_ping: SystemTime
}

impl<'a> Drop for WebSockets<'a> {
    fn drop(&mut self) {
        info!("Drop websocket");
        if let Some(ref mut socket) = self.socket {
            socket.0.close(None).unwrap()
        }
        self.disconnect().unwrap()
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
enum Events {
    BalanceUpdate(BalanceUpdateEvent),
    AccountUpdate(AccountUpdateEvent),
    OrderTrade(OrderTradeEvent),
    Trade(TradeEvent),
    Kline(KlineEvent),
}

impl<'a> WebSockets<'a> {
    pub fn new(testnet: bool, handler: Callback<'a>) -> WebSockets<'a> {
        WebSockets {
            socket: None,
            handler,
            testnet,
            last_ping: SystemTime::now()
        }
    }

    #[allow(dead_code)]
    pub fn connect(&mut self, subscription: &str) -> DreamrunnerResult<()> {
        self.connect_wss(&WebSocketAPI::Default.params(subscription, self.testnet))
    }

    pub fn connect_with_config(&mut self, subscription: &str, config: &Config) -> DreamrunnerResult<()> {
        self.connect_wss(
            &WebSocketAPI::Custom(config.ws_endpoint.clone()).params(subscription, self.testnet)
        )
    }

    pub fn connect_multiple_streams(&mut self, endpoints: &[String], testnet: bool) -> DreamrunnerResult<()> {
        self.connect_wss(&WebSocketAPI::MultiStream.params(&endpoints.join("/"), testnet))?;
        info!("Binance websocket connected");
        Ok(())
    }

    fn connect_wss(&mut self, wss: &str) -> DreamrunnerResult<()> {
        let url = Url::parse(wss)?;
        match connect(url) {
            Ok(answer) => {
                self.socket = Some(answer);
                Ok(())
            }
            Err(e) => Err(DreamrunnerError::TokioTungstenite(e)),
        }
    }

    pub fn disconnect(&mut self) -> DreamrunnerResult<()> {
        if let Some(ref mut socket) = self.socket {
            socket.0.close(None)?;
            return Ok(());
        }
        Err(DreamrunnerError::WebSocketDisconnected)
    }

    fn handle_msg(&mut self, msg: &str) -> DreamrunnerResult<()> {
        let value: serde_json::Value = serde_json::from_str(msg)?;
        if let Some(data) = value.get("data") {
            let msg = &data.to_string();
            let value: serde_json::Value = serde_json::from_str(msg)?;
            if let Ok(events) = serde_json::from_value::<Events>(value) {
                let action = match events {
                    Events::BalanceUpdate(v) => WebSocketEvent::BalanceUpdate(v),
                    Events::AccountUpdate(v) => WebSocketEvent::AccountUpdate(v),
                    Events::OrderTrade(v) => WebSocketEvent::OrderTrade(v),
                    Events::Trade(v) => WebSocketEvent::Trade(v),
                    Events::Kline(v) => WebSocketEvent::Kline(v),
                };
                (self.handler)(action)?;
            }
        }
        if let Ok(events) = serde_json::from_value::<Events>(value) {
            let action = match events {
                Events::BalanceUpdate(v) => WebSocketEvent::BalanceUpdate(v),
                Events::AccountUpdate(v) => WebSocketEvent::AccountUpdate(v),
                Events::OrderTrade(v) => WebSocketEvent::OrderTrade(v),
                Events::Trade(v) => WebSocketEvent::Trade(v),
                Events::Kline(v) => WebSocketEvent::Kline(v),
            };
            (self.handler)(action)?;
        }
        Ok(())
    }

    pub fn event_loop(&mut self, running: &AtomicBool) -> DreamrunnerResult<()> {
        while running.load(Ordering::Relaxed) {
            if let Some(ref mut socket) = self.socket {
                let now = SystemTime::now();
                // sending a ping to binance doesn't imply a pong will be received,
                // but it does keep Heroku from closing the websocket connection
                if now.duration_since(self.last_ping)?.as_secs() > 30 {
                    debug!("send ping");
                    socket.0.send(Message::Pong(vec![]))?;
                    self.last_ping = now;
                }
                
                match socket.0.read()? {
                    Message::Text(msg) => match self.handle_msg(&msg) {
                        Ok(_) => {}
                        Err(e) => {
                            if let DreamrunnerError::WebSocketDisconnected = e {
                                error!("Websocket disconnected: {:#?}", e);
                                return Err(e);
                            }
                        }
                    },
                    Message::Ping(msg) => {
                        debug!("recv ping");
                        let now = SystemTime::now();
                        match socket.0.send(Message::Pong(msg)) {
                            Ok(_) => {
                                info!("send pong after {}ms", now.elapsed()?.as_millis());
                            }
                            Err(e) => {
                                error!("Failed to reply with pong: {:#?}", e);
                                return Err(DreamrunnerError::TokioTungstenite(e))
                            },
                        }
                    }
                    Message::Pong(_) => {
                        info!("recv pong");
                    }
                    Message::Binary(_) | Message::Frame(_) => return Ok(()),
                    Message::Close(e) => {
                        return match e {
                            Some(e) => {
                                error!("Websocket closed: {:#?}", e);
                                Err(DreamrunnerError::Custom(e.to_string()))
                            },
                            None => Err(DreamrunnerError::WebSocketDisconnected),
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
