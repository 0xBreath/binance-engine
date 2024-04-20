mod plot;

use std::sync::Arc;
use actix_web::{get, web, App, HttpResponse, HttpServer, Responder};
use actix_web::web::Data;
use lib::*;
use dotenv::dotenv;
use log::*;
use simplelog::{ColorChoice, Config as SimpleLogConfig, TermLogger, TerminalMode};
use crate::plot::Plot;

// Binance spot TEST network
pub const BINANCE_TEST_API: &str = "https://testnet.binance.vision";
// Binance spot LIVE network
pub const BINANCE_LIVE_API: &str = "https://api.binance.us";
const BASE_ASSET: &str = "SOL";
const QUOTE_ASSET: &str = "USDT";
const TICKER: &str = "SOLUSDT";
const INTERVAL: Interval = Interval::FifteenMinutes;

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();
    init_logger();

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let bind_address = format!("0.0.0.0:{}", port);
    
    let account = match std::env::var("TESTNET")?.parse::<bool>()? {
        true => {
            Account {
                client: Client::new(
                    Some(std::env::var("BINANCE_TEST_API_KEY")?),
                    Some(std::env::var("BINANCE_TEST_API_SECRET")?),
                    BINANCE_TEST_API.to_string(),
                )?,
                recv_window: 5000,
                base_asset: BASE_ASSET.to_string(),
                quote_asset: QUOTE_ASSET.to_string(),
                ticker: TICKER.to_string(),
                interval: INTERVAL
            }
        }
        false => {
            Account {
                client: Client::new(
                    Some(std::env::var("BINANCE_LIVE_API_KEY")?),
                    Some(std::env::var("BINANCE_LIVE_API_SECRET")?),
                    BINANCE_LIVE_API.to_string(),
                )?,
                recv_window: 5000,
                base_asset: BASE_ASSET.to_string(),
                quote_asset: QUOTE_ASSET.to_string(),
                ticker: TICKER.to_string(),
                interval: INTERVAL
            }
        }
    };

    let state = Data::new(Arc::new(account));
    
    HttpServer::new(move || {
        App::new()
            .app_data(Data::clone(&state))
            .service(get_assets)
            .service(balance)
            .service(cancel_orders)
            .service(get_price)
            .service(exchange_info)
            .service(trades)
            .service(open_orders)
            .service(quote_pnl)
            .service(base_pnl)
            .service(pct_pnl)
            .service(pct_pnl_history)
            .service(quote_pnl_history)
            .service(base_pnl_history)
            .service(avg_trade_size)
            .service(klines)
            .route("/", web::get().to(test))
    })
    .bind(bind_address)?
    .run()
    .await
    .map_err(anyhow::Error::new)
}

fn init_logger() {
    TermLogger::init(
        LevelFilter::Info,
        SimpleLogConfig::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )
    .expect("Failed to initialize logger");
}

async fn test() -> impl Responder {
    HttpResponse::Ok().body("Server is running...")
}

#[get("/assets")]
async fn get_assets(account: Data<Arc<Account>>) -> DreamrunnerResult<HttpResponse> {
    let res = account.all_assets().await?;
    Ok(HttpResponse::Ok().json(res))
}

#[get("/balance")]
async fn balance(account: Data<Arc<Account>>) -> DreamrunnerResult<HttpResponse> {
    let assets = account.assets().await?;
    info!("Assets: {:#?}", assets);
    let price = account.price().await?;
    let balance = assets.balance(price);
    Ok(HttpResponse::Ok().json(balance))
}

#[get("/cancel")]
async fn cancel_orders(account: Data<Arc<Account>>) -> DreamrunnerResult<HttpResponse> {
    info!("Cancel all active orders");
    let res = account
        .cancel_all_open_orders().await?;
    let ids = res
        .iter()
        .map(|order| order.orig_client_order_id.clone().unwrap())
        .collect::<Vec<String>>();
    info!("All active orders canceled {:?}", ids);
    Ok(HttpResponse::Ok().json(res))
}

#[get("/price")]
async fn get_price(account: Data<Arc<Account>>) -> DreamrunnerResult<HttpResponse> {
    let res = account.price().await?;
    trace!("{:?}", res);
    Ok(HttpResponse::Ok().json(res))
}

#[get("/trades")]
async fn trades(account: Data<Arc<Account>>) -> DreamrunnerResult<HttpResponse> {
    info!("Fetching all historical orders...");
    let res = account
        .trades(account.ticker.clone()).await?;
    Ok(HttpResponse::Ok().json(res))
}

#[get("/openOrders")]
async fn open_orders(account: Data<Arc<Account>>) -> DreamrunnerResult<HttpResponse> {
    let res = account
        .open_orders(account.ticker.clone()).await?;
    info!("Open orders: {:?}", res);
    Ok(HttpResponse::Ok().json(res))
}

#[get("/info")]
async fn exchange_info(account: Data<Arc<Account>>) -> DreamrunnerResult<HttpResponse> {
    let info = account
        .exchange_info(account.ticker.clone()).await?;
    Ok(HttpResponse::Ok().json(info))
}

#[get("/avgTradeSize")]
async fn avg_trade_size(account: Data<Arc<Account>>) -> DreamrunnerResult<HttpResponse> {
    let avg = account.avg_quote_trade_size(account.ticker.clone()).await?;
    Ok(HttpResponse::Ok().json(avg))
}

#[get("/quotePnl")]
async fn quote_pnl(account: Data<Arc<Account>>) -> DreamrunnerResult<HttpResponse> {
    let res = account
      .quote_pnl(account.ticker.clone()).await?;
    Ok(HttpResponse::Ok().json(res))
}

#[get("/basePnl")]
async fn base_pnl(account: Data<Arc<Account>>) -> DreamrunnerResult<HttpResponse> {
    let res = account
      .base_pnl(account.ticker.clone()).await?;
    Ok(HttpResponse::Ok().json(res))
}

#[get("/percentPnl")]
async fn pct_pnl(account: Data<Arc<Account>>) -> DreamrunnerResult<HttpResponse> {
    let res = account
      .pct_pnl(account.ticker.clone()).await?;
    Ok(HttpResponse::Ok().json(res))
}

#[get("/percentPnlHistory")]
async fn pct_pnl_history(account: Data<Arc<Account>>) -> DreamrunnerResult<HttpResponse> {
    let data = account
      .cum_pct_pnl_history(account.ticker.clone()).await?;

    let out_file = &format!("{}/{}.png", env!("CARGO_MANIFEST_DIR"), "percent_pnl");
    Plot::plot(
        data,
        out_file,
        "Percent Pnl",
        "% ROI"
    )?;

    Ok(HttpResponse::Ok().body("Ok"))
}

#[get("/basePnlHistory")]
async fn base_pnl_history(account: Data<Arc<Account>>) -> DreamrunnerResult<HttpResponse> {
    let data = account
      .cum_base_pnl_history(account.ticker.clone()).await?;

    let out_file = &format!("{}/{}.png", env!("CARGO_MANIFEST_DIR"), "base_pnl");
    Plot::plot(
        data,
        out_file,
        "Base Pnl",
        BASE_ASSET
    )?;
    Ok(HttpResponse::Ok().body("Ok"))
}

#[get("/quotePnlHistory")]
async fn quote_pnl_history(account: Data<Arc<Account>>) -> DreamrunnerResult<HttpResponse> {
    let data = account
      .cum_quote_pnl_history(account.ticker.clone()).await?;

    let out_file = &format!("{}/{}.png", env!("CARGO_MANIFEST_DIR"), "quote_pnl");
    Plot::plot(
        data,
        out_file,
        "Quote Pnl",
        QUOTE_ASSET
    )?;
    Ok(HttpResponse::Ok().body("Ok"))
}

#[get("/klines")]
async fn klines(account: Data<Arc<Account>>) -> DreamrunnerResult<HttpResponse> {
    let res = account.klines(None, None, None).await?;
    Ok(HttpResponse::Ok().json(res))
}