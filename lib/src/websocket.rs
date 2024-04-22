#![allow(clippy::result_large_err)]

use std::future::Future;
use std::pin::Pin;
use crate::config::Config;
use crate::errors::{DreamrunnerError, DreamrunnerResult};
use crate::model::{
    AccountUpdateEvent, BalanceUpdateEvent, KlineEvent, OrderTradeEvent, TradeEvent,
};
use log::*;
use serde::{Deserialize, Serialize};
// use std::net::TcpStream;
use tokio::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};
use futures::{StreamExt, SinkExt};
use tokio::runtime::Handle;
// use tokio_tungstenite::tungstenite::http::Response;
use tokio_tungstenite::tungstenite::handshake::client::Response;
use tokio_tungstenite::tungstenite::protocol::{WebSocket, WebSocketConfig};
// use tokio_tungstenite::tungstenite::stream::MaybeTlsStream;
use tokio_tungstenite::MaybeTlsStream;
use tokio_tungstenite::tungstenite::{connect, Message};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::connect_async;
use url::Url;

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

// pub type Callback<'a> = Box<dyn FnMut(WebSocketEvent) -> DreamrunnerResult<()> + 'a>;
pub type Callback = Box<dyn FnMut(WebSocketEvent) -> Pin<Box<dyn Future<Output = DreamrunnerResult<()>> + Send>> + Sync>;

pub struct WebSockets {
    pub socket: Option<(WebSocketStream<MaybeTlsStream<TcpStream>>, Response)>,
    handler: Callback,
    testnet: bool,
    last_ping: SystemTime
}

impl Drop for WebSockets {
    fn drop(&mut self) {
        info!("Drop websocket");
        if let Some(ref mut socket) = self.socket {
            tokio::task::block_in_place(move || {
                Handle::current().block_on(async move {
                    socket.0.close(None).await.unwrap()
                })
            });
        }
        tokio::task::block_in_place(move || {
            Handle::current().block_on(async move {
                self.disconnect().await.unwrap()
            })
        });
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

impl WebSockets {
    pub fn new(testnet: bool, handler: Callback) -> WebSockets
    // where
        // C: FnMut(WebSocketEvent) -> Pin<Box<dyn Future<Output = DreamrunnerResult<()>> + Send>> + Sync + 'static,
        // C: FnMut(WebSocketEvent) -> DreamrunnerResult<()> + 'a,
    {
        WebSockets {
            socket: None,
            handler,
            testnet,
            last_ping: SystemTime::now()
        }
    }

    #[allow(dead_code)]
    pub async fn connect(&mut self, subscription: &str) -> DreamrunnerResult<()> {
        self.connect_wss(&WebSocketAPI::Default.params(subscription, self.testnet)).await
    }

    pub async fn connect_with_config(&mut self, subscription: &str, config: &Config) -> DreamrunnerResult<()> {
        self.connect_wss(
            &WebSocketAPI::Custom(config.ws_endpoint.clone()).params(subscription, self.testnet)
        ).await
    }

    pub async fn connect_multiple_streams(&mut self, endpoints: &[String], testnet: bool) -> DreamrunnerResult<()> {
        self.connect_wss(&WebSocketAPI::MultiStream.params(&endpoints.join("/"), testnet)).await?;
        info!("Binance websocket connected");
        Ok(())
    }

    async fn connect_wss(&mut self, wss: &str) -> DreamrunnerResult<()> {
        let url = Url::parse(wss)?;
        match connect_async(url).await {
            Ok(answer) => {
                self.socket = Some(answer);
                Ok(())
            }
            Err(e) => Err(DreamrunnerError::TokioTungstenite(e)),
        }
    }

    pub async fn disconnect(&mut self) -> DreamrunnerResult<()> {
        if let Some(ref mut socket) = self.socket {
            socket.0.close(None).await?;
            return Ok(());
        }
        Err(DreamrunnerError::WebSocketDisconnected)
    }

    async fn handle_msg(&mut self, msg: &str) -> DreamrunnerResult<()> {
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
                (self.handler)(action).await?;
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
            (self.handler)(action).await?;
        }
        Ok(())
    }

    pub async fn event_loop(&mut self, running: &AtomicBool) -> DreamrunnerResult<()> {
        while running.load(Ordering::Relaxed) {
            if let Some(ref mut socket) = self.socket {
                let now = SystemTime::now();
                // sending a ping to binance doesn't imply a pong will be received,
                // but it does keep Heroku from closing the websocket connection
                if now.duration_since(self.last_ping)?.as_secs() > 30 {
                    debug!("send ping");
                    // socket.0.write_message(Message::Pong(vec![]))?;
                    socket.0.send(Message::Ping(vec![])).await?;
                    self.last_ping = now;
                    
                }
                
                if let Some(msg) = socket.0.next().await {
                    match msg? {
                        Message::Text(msg) => match self.handle_msg(&msg).await {
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
                            match socket.0.send(Message::Pong(msg)).await {
                                Ok(_) => {
                                    info!("send pong");
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
        }
        Ok(())
    }
}
