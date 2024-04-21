mod alert;
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
use tokio::runtime::Handle;

// Binance spot TEST network
pub const BINANCE_TEST_API: &str = "https://testnet.binance.vision";
// Binance spot LIVE network
pub const BINANCE_LIVE_API: &str = "https://api.binance.us";
pub const BASE_ASSET: &str = "SOL";
pub const QUOTE_ASSET: &str = "USDT";
pub const TICKER: &str = "SOLUSDT";

#[actix_web::main]
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
    DreamrunnerResult::<_>::Ok(())
  });

  
  let (tx, rx) = crossbeam::channel::unbounded::<WebSocketEvent>();
  
  tokio::task::spawn(async move {
    let mut ws = WebSockets::new(testnet, |event: WebSocketEvent| {
      match event {
        WebSocketEvent::AccountUpdate(_) => {
          Ok(tx.send(event)?)
        }
        WebSocketEvent::OrderTrade(_) => {
          Ok(tx.send(event)?)
        }
        _ => Ok(()),
      }
    });

    let subs = vec![listen_key];
    while AtomicBool::new(true).load(Ordering::Relaxed) {
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
          if let Err(e) = ws.event_loop(&AtomicBool::new(true)) {
            error!("🛑Binance websocket error: {:#?}", e);
            tokio::task::block_in_place(move || {
              Handle::current().block_on(async move {
                tokio::time::sleep(Duration::from_secs(5)).await
              })
            });
          }
        }
      }
    }
    DreamrunnerResult::<_>::Ok(())
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
  );
  engine.ignition().await?;

  let state = Data::new(Arc::new(engine));

  HttpServer::new(move || {
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
    .run()
    .await
    .map_err(DreamrunnerError::from)
}

#[get("/")]
async fn test() -> DreamrunnerResult<HttpResponse> {
  Ok(HttpResponse::Ok().body("Relay is live!"))
}

#[post("/alert")]
async fn post_alert(state: Data<Arc<Engine>>, payload: Payload) -> DreamrunnerResult<HttpResponse> {
  let alert = match state.alert(payload).await {
    Ok(res) => Ok(res),
    Err(e) => {
      error!("{:?}", e);
      Err(e)
    }
  }?;
  info!("{:#?}", alert);
  Ok(HttpResponse::Ok().json(alert))
}