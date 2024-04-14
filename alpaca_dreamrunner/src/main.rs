#![allow(clippy::let_unit_value)]
#![allow(dead_code)]

mod endpoints;
mod engine;
mod error;
mod utils;

use apca::api::v2::updates::OrderUpdates;
use apca::data::v2::stream::{drive, Data};
use apca::data::v2::stream::{CustomUrl, MarketData};
use apca::data::v2::stream::{RealtimeData, IEX};
use apca::ApiInfo;
use apca::Client;
use crossbeam::channel::unbounded;
use endpoints::*;
use engine::*;
use ephemeris::*;
use error::*;
use futures::FutureExt as _;
use futures::TryStreamExt as _;
use lazy_static::lazy_static;
use log::*;
use std::path::PathBuf;
use std::sync::Mutex;
use time_series::{Candle, Day, Month, Time};
use utils::*;

/// Paper trading API credentials
pub const ALPACA_TEST_API_KEY: &str = "PKYIM924ABM29BNIRAXA";
pub const ALPACA_TEST_API_SECRET: &str = "gFedYbEywA95f41z5k7VTDkikAAYsZbnGgebfXHX";
/// Live trading API credentials
pub const ALPACA_LIVE_API_KEY: &str = "AK4ZHDVHCN9AZJSLKXET";
pub const ALPACA_LIVE_API_SECRET: &str = "9K0AZhmryDkiKzhI32xg8UvbbPs325MiAcu8pjhY";

lazy_static! {
    static ref API_INFO: ApiInfo =
        match is_testnet().expect("Failed to parse env TESTNET to boolean") {
            true => {
                ApiInfo::from_parts(
                    ALPACA_API_PAPER_URL,
                    ALPACA_TEST_API_KEY,
                    ALPACA_TEST_API_SECRET,
                ).unwrap()
            }
            false => {
                ApiInfo::from_parts(
                    ALPACA_API_LIVE_URL,
                    ALPACA_LIVE_API_KEY,
                    ALPACA_LIVE_API_SECRET,
                ).unwrap()
            }
        };
    // cache previous and current Kline/Candle to assess PLPL trade signal
    static ref PREV_CANDLE: Mutex<Option<Candle>> = Mutex::new(None);
    static ref CURR_CANDLE: Mutex<Option<Candle>> = Mutex::new(None);
    static ref TICKER: String = "SPY".to_string();
}

#[tokio::main]
async fn main() -> Result<()> {
    init_logger(&PathBuf::from("alpaca.log"))?;

    // PLPL parameters; tuned for 5 minute candles
    let trailing_take_profit = ExitType::Price(3.5);
    let stop_loss = ExitType::Percent(0.5);
    let planet = Planet::from("Jupiter");
    let plpl_scale = 0.5;
    let plpl_price = 400.0;
    let num_plpls = 8000;
    let cross_margin_pct = 55.0;

    // initialize PLPL
    let plpl_system = PLPLSystem::new(PLPLSystemConfig {
        planet,
        origin: Origin::Heliocentric,
        first_date: Time::new(2023, &Month::from_num(9), &Day::from_num(1), None, None),
        last_date: Time::new(2050, &Month::from_num(9), &Day::from_num(1), None, None),
        plpl_scale,
        plpl_price,
        num_plpls,
        cross_margin_pct,
    })?;

    let client = Client::new(API_INFO.clone());

    // PLPL engine
    let engine = Engine::new(
        client,
        TICKER.clone(),
        plpl_system,
        trailing_take_profit,
        stop_loss,
    );

    // before starting to process messages, reset open orders and equalize assets
    engine.cancel_open_orders().await?;
    engine.equalize_assets().await?;

    // Subscribe to websocket bar updates.
    let (mut stream, mut subscription) = engine.client.subscribe::<RealtimeData<IEX>>().await?;
    let mut data = MarketData::default();
    data.set_bars([TICKER.as_str()]);
    let subscribe = subscription.subscribe(&data).boxed();
    let () = drive(subscribe, &mut stream).await?.unwrap()?;

    // Subscribe to websocket order updates.
    let (stream_orders, _subscription_orders) = engine.client.subscribe::<OrderUpdates>().await?;

    let engine = Mutex::new(engine);
    let (tx, rx) = unbounded::<WebSocketEvent>();

    // handle websocket order updates
    let order_sender = tx.clone();
    tokio::spawn(async move {
        match stream_orders
            .map_err(AlpacaError::WebSocket)
            .try_for_each(|result| async {
                let data = result.map_err(AlpacaError::Json)?;
                let suffix = order_id_suffix(&data.order);
                if suffix == "ENTRY" || suffix == "TAKE_PROFIT" || suffix == "STOP_LOSS" {
                    order_sender.send(WebSocketEvent::OrderUpdate(data))?;
                }
                Ok(())
            })
            .await
        {
            Ok(()) => Ok(()),
            Err(e) => {
                error!("Error in order stream: {:?}", e);
                Err(e)
            }
        }
    });

    // handle websocket bar updates
    let bar_sender = tx.clone();
    tokio::spawn(async move {
        match stream
            .map_err(AlpacaError::WebSocket)
            .try_for_each(|result| async {
                let data = result.map_err(AlpacaError::Json)?;

                match data {
                    Data::Quote(quote) => {
                        trace!("quote: {:?}", quote);
                    }
                    Data::Trade(trade) => {
                        trace!("{:?}", trade);
                    }
                    Data::Bar(bar) => {
                        bar_sender.send(WebSocketEvent::Bar(bar))?;
                    }
                    _ => {
                        trace!("Other websocket data: {:?}", data);
                    }
                }
                Ok(())
            })
            .await
        {
            Ok(()) => Ok(()),
            Err(e) => {
                error!("Error in bar stream: {:?}", e);
                Err(e)
            }
        }
    });

    // handle queue messages
    while let Ok(event) = rx.recv() {
        match event {
            WebSocketEvent::Bar(bar) => {
                trace!("bar: {:?}", bar);

                let mut prev = PREV_CANDLE.lock()?;
                let mut curr = CURR_CANDLE.lock()?;
                let mut engine = engine.lock()?;

                // compute closest PLPL to current Candle
                let candle = bar_to_candle(bar)?;
                // compare previous candle to current candle to check crossover of PLPL signal threshold
                match (&*prev, &*curr) {
                    (None, None) => *prev = Some(candle),
                    (Some(prev_candle), None) => {
                        *curr = Some(candle.clone());

                        engine
                            .process_candle(
                                prev_candle,
                                &candle,
                                candle.date.to_unix_ms().to_string(),
                            )
                            .await?;
                    }
                    (None, Some(_)) => {
                        error!(
                            "🛑 Previous candle is None and current candle is Some. Should never occur."
                        );
                    }
                    (Some(_prev_candle), Some(curr_candle)) => {
                        engine
                            .process_candle(
                                curr_candle,
                                &candle,
                                candle.date.to_unix_ms().to_string(),
                            )
                            .await?;

                        *prev = Some(curr_candle.clone());
                        *curr = Some(candle);
                    }
                }
            }
            WebSocketEvent::OrderUpdate(order_update) => {
                let mut engine = engine.lock()?;

                engine.update_active_order(order_update)?;
                engine.check_active_order().await?;
            }
        }
    }

    Ok(())
}
