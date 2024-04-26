mod engine;
mod utils;
use engine::*;
use utils::*;

use lib::*;
use dotenv::dotenv;
use log::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use playbook::Dreamrunner;


// Binance spot TEST network
pub const BINANCE_TEST_API: &str = "https://testnet.binance.vision";
// Binance spot LIVE network
pub const BINANCE_LIVE_API: &str = "https://api.binance.us";
pub const KLINE_STREAM: &str = "solusdt@kline_30m";
pub const INTERVAL: Interval = Interval::ThirtyMinutes;
pub const BASE_ASSET: &str = "SOL";
pub const QUOTE_ASSET: &str = "USDT";
pub const TICKER: &str = "SOLUSDT";

#[tokio::main]
async fn main() -> DreamrunnerResult<()> {
  dotenv().ok();
  init_logger()?;

  let binance_test_api_key = std::env::var("BINANCE_TEST_API_KEY")?;
  let binance_test_api_secret = std::env::var("BINANCE_TEST_API_SECRET")?;
  let binance_live_api_key = std::env::var("BINANCE_LIVE_API_KEY")?;
  let binance_live_api_secret = std::env::var("BINANCE_LIVE_API_SECRET")?;

  let equity_pct = 90.0;
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

  let (tx, rx) = crossbeam::channel::unbounded::<WebSocketEvent>();

  let mut engine = Engine::new(
    client.clone(),
    rx,
    disable_trading,
    BASE_ASSET.to_string(),
    QUOTE_ASSET.to_string(),
    TICKER.to_string(),
    INTERVAL,
    min_notional,
    equity_pct,
    Dreamrunner::solusdt_optimized()
  );

  let running = Arc::new(AtomicBool::new(true));

  let ws_running = running.clone();
  tokio::task::spawn(async move {
    let callback: Callback = Box::new(move |event: WebSocketEvent| {
      match event {
        WebSocketEvent::Kline(_) => {
          DreamrunnerResult::<_>::Ok(tx.send(event)?)
        }
        WebSocketEvent::AccountUpdate(_) => {
          DreamrunnerResult::<_>::Ok(tx.send(event)?)
        }
        WebSocketEvent::OrderTrade(_) => {
          DreamrunnerResult::<_>::Ok(tx.send(event)?)
        }
        _ => DreamrunnerResult::<_>::Ok(()),
      }
    });
    let mut ws = WebSockets::new(testnet, client, callback);

    while ws_running.load(Ordering::Relaxed) {
      // reconnect user stream and update listen key
      ws.connect_user_stream().await?;

      // reconnect Binance websocket
      let subs = vec![KLINE_STREAM.to_string(), ws.listen_key.clone()];
      match ws.connect_multiple_streams(&subs, testnet).await {
        Err(e) => {
          error!("🛑Failed to connect websocket: {}", e);
        }
        Ok(_) => {
          // if user stream is disconnected it will set `is_connected` to false which will break the event loop.
          // then this outer while loop will literate and reconnect the user stream and websocket
          match ws.event_loop().await {
            Err(e) => error!("🛑Websocket error: {:#?}", e),
            Ok(_) => warn!("🟡 Websocket needs to reconnect")
          }
        }
      }
    }
    warn!("🟡 Shutting down websocket stream");
    ws.disconnect().await?;
    ws.disconnect_user_stream().await?;

    DreamrunnerResult::<_>::Ok(())
  });

  // start engine that listens to websocket updates (candles, account balance updates, trade updates)
  engine.ignition().await?;

  // wait for ctrl-c SIGINT to execute graceful shutdown
  tokio::signal::ctrl_c().await?;
  warn!("🟡 Shutting down Dreamrunner...");
  running.store(false, Ordering::Relaxed);

  Ok(())
}