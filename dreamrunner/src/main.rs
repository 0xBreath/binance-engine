use lib::*;
use dotenv::dotenv;
use log::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::runtime::Handle;
use time_series::{trunc, Time};

mod engine;
mod utils;
mod kagi;
mod dreamrunner;
use engine::*;
use utils::*;
use crate::dreamrunner::Dreamrunner;

// Binance spot TEST network
pub const BINANCE_TEST_API: &str = "https://testnet.binance.vision";
// Binance spot LIVE network
pub const BINANCE_LIVE_API: &str = "https://api.binance.us";
pub const KLINE_STREAM: &str = "solusdt@kline_30m";
pub const BASE_ASSET: &str = "SOL";
pub const QUOTE_ASSET: &str = "USDT";
pub const TICKER: &str = "SOLUSDT";

#[tokio::main]
async fn main() -> DreamrunnerResult<()> {
  dotenv().ok();
  init_logger(&PathBuf::from("dreamrunner.log".to_string()))?;
  info!("Starting Binance Dreamrunner!");

  let binance_test_api_key = std::env::var("BINANCE_TEST_API_KEY")?;
  let binance_test_api_secret = std::env::var("BINANCE_TEST_API_SECRET")?;
  let binance_live_api_key = std::env::var("BINANCE_LIVE_API_KEY")?;
  let binance_live_api_secret = std::env::var("BINANCE_LIVE_API_SECRET")?;

  let wma_period = 5;
  let equity_pct = 95.0;
  let min_notional = 5.0; // $5 USD is the minimum SOL that can be traded

  let testnet = is_testnet()?;
  let disable_trading = disable_trading()?;

  let client = match is_testnet()? {
    true => Client::new(
        Some(binance_test_api_key.to_string()),
        Some(binance_test_api_secret.to_string()),
        BINANCE_TEST_API.to_string(),
      )?,
    false => Client::new(
        Some(binance_live_api_key.to_string()),
        Some(binance_live_api_secret.to_string()),
        BINANCE_LIVE_API.to_string(),
      )?
  };

  let user_stream = UserStream {
    client: client.clone(),
    recv_window: 10000,
  };
  let answer = user_stream.start().await?;
  let listen_key = answer.listen_key;

  let running = AtomicBool::new(true);
  let listen_key_copy = listen_key.clone();
  tokio::task::spawn(async move {
    let mut last_ping = SystemTime::now();

    while running.load(Ordering::Relaxed) {
      let now = SystemTime::now();
      // check if timestamp is 30 seconds after last UserStream keep alive ping
      let elapsed = now.duration_since(last_ping).map(|d| d.as_secs())?;

      if elapsed > 30 {
        if let Err(e) = user_stream.keep_alive(&listen_key_copy).await {
          error!("🛑Error on user stream keep alive: {}", e);
        }
        last_ping = now;
      }
      tokio::time::sleep(Duration::new(1, 0)).await;
    }
    Result::<_, anyhow::Error>::Ok(())
  });

  let (tx, rx) = crossbeam::channel::unbounded::<WebSocketEvent>();

  tokio::task::spawn(async move {
    let mut ws = WebSockets::new(testnet, |event: WebSocketEvent| {
      match event {
        WebSocketEvent::Kline(_) => {
          Ok(tx.send(event)?)
        }
        WebSocketEvent::AccountUpdate(_) => {
          Ok(tx.send(event)?)
        }
        WebSocketEvent::OrderTrade(_) => {
          Ok(tx.send(event)?)
        }
        _ => Ok(()),
      }
    });

    let subs = vec![KLINE_STREAM.to_string(), listen_key];
    match ws.connect_multiple_streams(&subs, testnet) {
      Err(e) => {
        error!("🛑Failed to connect Binance websocket: {}", e);
        Err(e)
      }
      Ok(_) => {
        info!("Binance websocket connected");
        Ok(())
      },
    }?;

    if let Err(e) = ws.event_loop(&AtomicBool::new(true)) {
      error!("🛑Binance websocket error: {:#?}", e);
      match ws.connect_multiple_streams(&subs, testnet) {
        Err(e) => {
          error!("🛑Failed to reconnect Binance websocket: {}", e);
          Err(e)
        }
        Ok(_) => {
          info!("Reconnected Binance websocket");
          Ok(())
        },
      }?;
    };

    Result::<_, anyhow::Error>::Ok(())
  });

  let mut engine = Engine::new(
    client,
    rx,
    disable_trading,
    BASE_ASSET.to_string(),
    QUOTE_ASSET.to_string(),
    TICKER.to_string(),
    min_notional,
    equity_pct,
    wma_period,
    Dreamrunner::default()
  );
  engine.ignition().await?;

  Ok(())
}
