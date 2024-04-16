use binance_lib::*;
use dotenv::dotenv;
use log::*;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use tokio::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
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

  let user_stream: Mutex<UserStream> =
    match is_testnet().expect("Failed to parse env TESTNET to boolean") {
      true => Mutex::new(UserStream {
        client: client.clone(),
        recv_window: 10000,
      }),
      false => Mutex::new(UserStream {
        client: client.clone(),
        recv_window: 10000,
      }),
    };

  let mut engine = Engine::new(
    client,
    10000,
    disable_trading,
    BASE_ASSET.to_string(),
    QUOTE_ASSET.to_string(),
    TICKER.to_string(),
    equity_pct,
    wma_period,
    Dreamrunner::default()
  );

  let user_stream_keep_alive_time = Mutex::new(SystemTime::now());
  let user_stream = user_stream.lock().await;
  let answer = user_stream.start().await?;
  let listen_key = answer.listen_key;

  // cancel all open orders to start with a clean slate
  engine.cancel_all_open_orders().await?;
  // equalize base and quote assets to 50/50
  if !disable_trading {
    engine.equalize_assets().await?;
  }
  // get initial asset balances
  engine.update_assets().await?;
  engine.log_assets();
  
  let engine = Mutex::new(engine);
  let mut ws = WebSockets::new(testnet, |event: WebSocketEvent| {
    let now = SystemTime::now();
    let mut keep_alive = tokio::task::block_in_place(|| {
      Handle::current().block_on(async {
        user_stream_keep_alive_time.lock().await
      })
    });
    // check if timestamp is 10 minutes after last UserStream keep alive ping
    let secs_since_keep_alive = now.duration_since(*keep_alive).map(|d| d.as_secs())?;

    if secs_since_keep_alive > 30 * 60 {
      let status = tokio::task::block_in_place(|| {
        Handle::current().block_on(async {
          user_stream.keep_alive(&listen_key).await
        })
      });
      match status {
        Ok(_) => {
          let now = Time::from_unix_msec(
            now.duration_since(UNIX_EPOCH).unwrap().as_millis() as i64,
          );
          info!("Keep alive user stream @ {}", now.to_string())
        }
        Err(e) => error!("🛑 Error on user stream keep alive: {}", e),
      }
      *keep_alive = now;
    }
    drop(keep_alive);

    let mut engine = tokio::task::block_in_place(|| {
      Handle::current().block_on(async {
        engine.lock().await
      })
    });

    match event {
      WebSocketEvent::Kline(kline_event) => {
        // only accept if this candle is at the end of the bar period
        if kline_event.kline.is_final_bar {
          let candle = kline_to_candle(&kline_event)?;
          info!("{:#?}", kline_event.kline.info()?);
          tokio::task::block_in_place( || {
            Handle::current().block_on(async {
              engine.process_candle(candle).await
            })
          })?;
        }
      }
      WebSocketEvent::AccountUpdate(account_update) => {
        let assets = account_update.assets(&engine.quote_asset, &engine.base_asset)?;
        info!(
            "Account Update, {}: {}, {}: {}",
            engine.quote_asset, assets.free_quote, engine.base_asset, assets.free_base
        );
      }
      WebSocketEvent::OrderTrade(event) => {
        let order_type = ActiveOrder::client_order_id_suffix(&event.new_client_order_id);
        let entry_price = trunc!(event.price.parse::<f64>()?, 2);
        info!(
            "{},  {},  {} @ {},  Execution: {},  Status: {},  Order: {}",
            event.symbol,
            event.new_client_order_id,
            event.side,
            entry_price,
            event.execution_type,
            event.order_status,
            order_type
        );
        // update state
        engine.update_active_order(event)?;
        // create or cancel orders depending on state
        tokio::task::block_in_place(|| {
          Handle::current().block_on(async move {
            engine.check_active_order().await
          })
        })?;
      }
      _ => (),
    };
    Ok(())
  });

  let subs = vec![KLINE_STREAM.to_string(), listen_key.clone()];
  match ws.connect_multiple_streams(&subs, testnet) {
    Err(e) => {
      error!("🛑 Failed to connect to Binance websocket: {}", e);
      Err(e)
    }
    Ok(_) => {
      info!("Binance websocket connected");
      Ok(())
    },
  }?;
  
  match ws.event_loop(&AtomicBool::new(true)) {
    Err(e) => {
      error!("🛑 Binance websocket error: {:#?}", e);
      Err(e)
    },
    Ok(_) => Ok(())
  }?;

  Ok(())

  // user_stream.close(&listen_key)?;
  //
  // match ws.disconnect() {
  //     Err(e) => {
  //         error!("🛑 Failed to disconnect from Binance websocket: {}", e);
  //         match ws.connect_multiple_streams(&subs, testnet) {
  //             Err(e) => {
  //                 error!("🛑 Failed to connect to Binance websocket: {}", e);
  //                 Err(e)
  //             }
  //             Ok(_) => {
  //                 info!("Binance websocket reconnected");
  //                 Ok(())
  //             }
  //         }
  //     }
  //     Ok(_) => {
  //         warn!("Binance websocket disconnected");
  //         Ok(())
  //     }
  // }
}
