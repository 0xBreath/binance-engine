use std::sync::Arc;
use actix_web::{get, web, App, HttpResponse, HttpServer, Responder};
use actix_web::web::Data;
use lib::*;
use dotenv::dotenv;
use log::*;
use plotters::prelude::*;
use plotters::prelude::full_palette::DEEPPURPLE_A100;
use simplelog::{ColorChoice, Config as SimpleLogConfig, TermLogger, TerminalMode};

// Binance spot TEST network
pub const BINANCE_TEST_API: &str = "https://testnet.binance.vision";
// Binance spot LIVE network
pub const BINANCE_LIVE_API: &str = "https://api.binance.us";
const BASE_ASSET: &str = "SOL";
const QUOTE_ASSET: &str = "USDT";
const TICKER: &str = "SOLUSDT";
const INTERVAL: &str = "15m";

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
                interval: INTERVAL.to_string()
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
                interval: INTERVAL.to_string()
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
    trace!("{:?}", res);
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

    let mut min_x = i64::MAX;
    let mut max_x = i64::MIN;
    let mut min_y = f64::MAX;
    let mut max_y = f64::MIN;
    for datum in data.iter() {
        if datum.x < min_x {
            min_x = datum.x;
        }
        if datum.x > max_x {
            max_x = datum.x;
        }
        if datum.y < min_y {
            min_y = datum.y;
        }
        if datum.y > max_y {
            max_y = datum.y;
        }
    }

    let out_file = &format!("{}/pct_pnl_history.png", env!("CARGO_MANIFEST_DIR"));
    let root = BitMapBackend::new(out_file, (2048, 1024)).into_drawing_area();
    root.fill(&WHITE).map_err(
        |e| anyhow::anyhow!("Failed to fill drawing area with white: {}", e)
    )?;
    let mut chart = ChartBuilder::on(&root)
      .margin_top(20)
      .margin_bottom(20)
      .margin_left(30)
      .margin_right(30)
      .set_all_label_area_size(130)
      .caption(
          "Percent PnL History",
          ("sans-serif", 40.0).into_font(),
      )
      .build_cartesian_2d(min_x..max_x, min_y..max_y).map_err(
        |e| anyhow::anyhow!("Failed to build cartesian 2d: {}", e)
    )?;
    chart
      .configure_mesh()
      .light_line_style(WHITE)
      .label_style(("sans-serif", 30, &BLACK).into_text_style(&root))
      .x_desc("UNIX milliseconds")
      .y_desc("Percent PnL")
      .draw().map_err(
        |e| anyhow::anyhow!("Failed to draw mesh: {}", e)
    )?;

    // get color from colors array
    let color = RGBAColor::from(DEEPPURPLE_A100);

    chart.draw_series(
        LineSeries::new(
            data.iter().map(|data| (data.x, data.y)),
            ShapeStyle {
                color,
                filled: true,
                stroke_width: 2,
            },
        )
          .point_size(3),
    ).map_err(
        |e| anyhow::anyhow!("Failed to draw series: {}", e)
    )?;

    root.present().map_err(
        |e| anyhow::anyhow!("Failed to present root: {}", e)
    )?;

    Ok(HttpResponse::Ok().body("Ok"))
}

#[get("/basePnlHistory")]
async fn base_pnl_history(account: Data<Arc<Account>>) -> DreamrunnerResult<HttpResponse> {
    let data = account
      .cum_base_pnl_history(account.ticker.clone()).await?;

    let mut min_x = i64::MAX;
    let mut max_x = i64::MIN;
    let mut min_y = f64::MAX;
    let mut max_y = f64::MIN;
    for datum in data.iter() {
        if datum.x < min_x {
            min_x = datum.x;
        }
        if datum.x > max_x {
            max_x = datum.x;
        }
        if datum.y < min_y {
            min_y = datum.y;
        }
        if datum.y > max_y {
            max_y = datum.y;
        }
    }

    let out_file = &format!("{}/base_pnl_history.png", env!("CARGO_MANIFEST_DIR"));
    let root = BitMapBackend::new(out_file, (2048, 1024)).into_drawing_area();
    root.fill(&WHITE).map_err(
        |e| anyhow::anyhow!("Failed to fill drawing area with white: {}", e)
    )?;
    let mut chart = ChartBuilder::on(&root)
      .margin_top(20)
      .margin_bottom(20)
      .margin_left(30)
      .margin_right(30)
      .set_all_label_area_size(130)
      .caption(
          "Base PnL History",
          ("sans-serif", 40.0).into_font(),
      )
      .build_cartesian_2d(min_x..max_x, min_y..max_y).map_err(
        |e| anyhow::anyhow!("Failed to build cartesian 2d: {}", e)
    )?;
    chart
      .configure_mesh()
      .light_line_style(WHITE)
      .label_style(("sans-serif", 30, &BLACK).into_text_style(&root))
      .x_desc("UNIX milliseconds")
      .y_desc("Base PnL")
      .draw().map_err(
        |e| anyhow::anyhow!("Failed to draw mesh: {}", e)
    )?;

    // get color from colors array
    let color = RGBAColor::from(DEEPPURPLE_A100);

    chart.draw_series(
        LineSeries::new(
            data.iter().map(|data| (data.x, data.y)),
            ShapeStyle {
                color,
                filled: true,
                stroke_width: 2,
            },
        )
          .point_size(3),
    ).map_err(
        |e| anyhow::anyhow!("Failed to draw series: {}", e)
    )?;

    root.present().map_err(
        |e| anyhow::anyhow!("Failed to present root: {}", e)
    )?;

    Ok(HttpResponse::Ok().body("Ok"))
}

#[get("/quotePnlHistory")]
async fn quote_pnl_history(account: Data<Arc<Account>>) -> DreamrunnerResult<HttpResponse> {
    let data = account
      .cum_quote_pnl_history(account.ticker.clone()).await?;
    
    let mut min_x = i64::MAX;
    let mut max_x = i64::MIN;
    let mut min_y = f64::MAX;
    let mut max_y = f64::MIN;
    for datum in data.iter() {
        if datum.x < min_x {
            min_x = datum.x;
        }
        if datum.x > max_x {
            max_x = datum.x;
        }
        if datum.y < min_y {
            min_y = datum.y;
        }
        if datum.y > max_y {
            max_y = datum.y;
        }
    }

    let out_file = &format!("{}/quote_pnl_history.png", env!("CARGO_MANIFEST_DIR"));
    let root = BitMapBackend::new(out_file, (2048, 1024)).into_drawing_area();
    root.fill(&WHITE).map_err(
        |e| anyhow::anyhow!("Failed to fill drawing area with white: {}", e)
    )?;
    let mut chart = ChartBuilder::on(&root)
      .margin_top(20)
      .margin_bottom(20)
      .margin_left(30)
      .margin_right(30)
      .set_all_label_area_size(130)
      .caption(
          "Quote PnL History",
          ("sans-serif", 40.0).into_font(),
      )
      .build_cartesian_2d(min_x..max_x, min_y..max_y).map_err(
          |e| anyhow::anyhow!("Failed to build cartesian 2d: {}", e)
      )?;
    chart
      .configure_mesh()
      .light_line_style(WHITE)
      .label_style(("sans-serif", 30, &BLACK).into_text_style(&root))
      .x_desc("UNIX milliseconds")
      .y_desc("Quote PnL")
      .draw().map_err(
          |e| anyhow::anyhow!("Failed to draw mesh: {}", e)
      )?;

    // get color from colors array
    let color = RGBAColor::from(DEEPPURPLE_A100);
    
    chart.draw_series(
        LineSeries::new(
            data.iter().map(|data| (data.x, data.y)),
            ShapeStyle {
                color,
                filled: true,
                stroke_width: 2,
            },
        )
          .point_size(3),
    ).map_err(
        |e| anyhow::anyhow!("Failed to draw series: {}", e)
    )?;

    root.present().map_err(
        |e| anyhow::anyhow!("Failed to present root: {}", e)
    )?;
    
    Ok(HttpResponse::Ok().body("Ok"))
}

#[get("/klines")]
async fn klines(account: Data<Arc<Account>>) -> DreamrunnerResult<HttpResponse> {
    let res = account.klines().await?;
    Ok(HttpResponse::Ok().json(res))
}