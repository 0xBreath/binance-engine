mod engine;
mod utils;

use engine::*;
use utils::*;

use lib::*;
use log::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};
use dotenv::dotenv;
use actix_cors::Cors;
use actix_web::{
  get, post,
  web::{Data, Payload},
  App, HttpResponse, HttpServer,
};
use crossbeam::channel::Sender;
use tokio::runtime::Handle;

// Binance spot TEST network
pub const BINANCE_TEST_API: &str = "https://testnet.binance.vision";
// Binance spot LIVE network
pub const BINANCE_LIVE_API: &str = "https://api.binance.us";
pub const BASE_ASSET: &str = "SOL";
pub const QUOTE_ASSET: &str = "USDT";
pub const TICKER: &str = "SOLUSDT";

#[tokio::main]
async fn main() -> DreamrunnerResult<()> {
  dotenv().ok();
  init_logger()?;

  let port = std::env::var("PORT").unwrap_or_else(|_| "4444".to_string());
  let bind_address = format!("0.0.0.0:{}", port);

  info!("Starting Binance Relay!");

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

  // controller to kill as tokio tasks on SIGINT
  let running = Arc::new(AtomicBool::new(true));

  let user_stream = UserStream {
    client: client.clone(),
    recv_window: 10000,
  };
  let answer = user_stream.start().await?;
  let listen_key = answer.listen_key;

  let user_stream_running = running.clone();
  let listen_key_copy = listen_key.clone();
  let user_stream_handle = tokio::task::spawn(async move {
    let mut last_ping = SystemTime::now();

    while user_stream_running.load(Ordering::Relaxed) {
      let now = SystemTime::now();
      // check if timestamp is 30 seconds after last UserStream keep alive ping
      let elapsed = now.duration_since(last_ping).map(|d| d.as_secs())?;

      if elapsed > 30 {
        if let Err(e) = user_stream.keep_alive(&listen_key_copy).await {
          error!("🛑Error on user stream keep alive: {}", e);
        }
        last_ping = now;
      }
      tokio::time::sleep(Duration::from_secs(1)).await;
    }
    info!("⚠️ Shutting down user stream");
    DreamrunnerResult::<_>::Ok(())
  });

  
  let (tx, rx) = crossbeam::channel::unbounded::<ChannelMsg>();

  let ws_running = running.clone();
  let ws_tx = tx.clone();
  let ws_handle = tokio::task::spawn(async move {
    let callback: Callback = Box::new(|event: WebSocketEvent| {
      match event {
        WebSocketEvent::AccountUpdate(_) => {
          let msg = ChannelMsg::Websocket(event);
          ws_tx.send(msg)?
        }
        WebSocketEvent::OrderTrade(_) => {
          let msg = ChannelMsg::Websocket(event);
          ws_tx.send(msg)?
        }
        _ => (),
      };
      DreamrunnerResult::<_>::Ok(())
    });
    
    let mut ws = WebSockets::new(testnet, callback);

    let subs = vec![listen_key];
    while ws_running.load(Ordering::Relaxed) {
      match ws.connect_multiple_streams(&subs, testnet) {
        Err(e) => {
          error!("🛑Failed to connect Binance websocket: {}", e);
          tokio::task::block_in_place(move || {
            Handle::current().block_on(async move {
              tokio::time::sleep(Duration::from_secs(5)).await
            })
          });
        }
        Ok(_) => {
          if let Err(e) = ws.event_loop(&ws_running) {
            error!("🛑Binance websocket error: {:#?}", e);
            tokio::task::block_in_place(move || {
              Handle::current().block_on(async move {
                tokio::time::sleep(Duration::from_secs(5)).await
              })
            });
          }
          warn!("Finished event loop");
        }
      }
    }
    info!("⚠️ Shutting down websocket listener");
    ws.disconnect()?;
    DreamrunnerResult::<_>::Ok(())
  });

  let engine_handle = tokio::task::spawn(async move {
    let mut engine = Engine::new(
      client,
      rx,
      disable_trading,
      BASE_ASSET.to_string(),
      QUOTE_ASSET.to_string(),
      TICKER.to_string(),
      min_notional,
      equity_pct,
    );
    engine.ignition().await?;
    info!("⚠️ Shutting down engine");
    DreamrunnerResult::<_>::Ok(())
  });

  let state = Data::new(Arc::new(tx));

  let server = HttpServer::new(move || {
    let cors = Cors::default()
      .allow_any_origin()
      .allowed_methods(vec!["GET", "POST"])
      .allow_any_header()
      .max_age(3600);

    App::new()
      .app_data(Data::clone(&state))
      .wrap(cors)
      .service(test)
      .service(post_alert)
  })
    .bind(bind_address)?
    .run();

  let server_handle = tokio::task::spawn(async move {
    server.await?;
    info!("⚠️ Shutting down server");
    DreamrunnerResult::<_>::Ok(())
  });
  
  tokio::signal::ctrl_c().await?;
  warn!("SIGINT received, shutting down");
  running.store(false, Ordering::Relaxed);
  // std::process::exit(0);

  let _ = ws_handle.await?;
  let _ = engine_handle.await?;
  let _ = user_stream_handle.await?;
  let _ = server_handle.await?;

  Ok(())
}

#[get("/")]
async fn test() -> DreamrunnerResult<HttpResponse> {
  Ok(HttpResponse::Ok().body("Relay is live!"))
}

#[post("/alert")]
async fn post_alert(state: Data<Arc<Sender<ChannelMsg>>>, payload: Payload) -> DreamrunnerResult<HttpResponse> {
  let alert = Engine::alert(payload).await?;
  state.send(ChannelMsg::Alert(alert))?;

  Ok(HttpResponse::Ok().body("Ok"))
}