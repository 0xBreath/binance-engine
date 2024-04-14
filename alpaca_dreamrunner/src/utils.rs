use crate::{AlpacaError, Result};
use apca::api::v2::order::{Order, Side, Status};
use apca::api::v2::updates::OrderUpdate;
use apca::data::v2::stream::Bar;
use log::*;
use num_decimal::Num;
use simplelog::{
    ColorChoice, CombinedLogger, Config as SimpleLogConfig, ConfigBuilder, TermLogger,
    TerminalMode, WriteLogger,
};
use std::fs::File;
use std::path::PathBuf;
use std::str::FromStr;
use time_series::{f64_to_num, num_to_f64, precise_round, Candle, Time};

pub fn init_logger(log_file: &PathBuf) -> Result<()> {
    let level_env = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    let level = LevelFilter::from_str(&level_env)?;
    CombinedLogger::init(vec![
        TermLogger::new(
            level,
            SimpleLogConfig::default(),
            TerminalMode::Mixed,
            ColorChoice::Always,
        ),
        WriteLogger::new(
            level,
            ConfigBuilder::new().set_time_format_rfc3339().build(),
            File::create(log_file)?,
        ),
    ])
    .map_err(AlpacaError::Logger)
}

pub fn bar_to_candle(bar: Bar) -> Result<Candle> {
    Ok(Candle {
        date: Time::from_datetime(bar.timestamp),
        open: num_to_f64!(bar.open_price)?,
        high: num_to_f64!(bar.high_price)?,
        low: num_to_f64!(bar.low_price)?,
        close: num_to_f64!(bar.close_price)?,
        volume: None,
    })
}

pub fn is_testnet() -> Result<bool> {
    std::env::var("TESTNET")?
        .parse::<bool>()
        .map_err(AlpacaError::ParseBool)
}

pub fn order_id_prefix(order: &Order) -> String {
    order.client_order_id.split('-').next().unwrap().to_string()
}

pub fn order_id_suffix(order: &Order) -> String {
    order.client_order_id.split('-').last().unwrap().to_string()
}

#[derive(Debug, Clone)]
pub enum ExitType {
    Percent(f64),
    Price(f64),
}

impl ExitType {
    pub fn calc_stop_loss_exit(&self, entry_side: &Side, origin: f64) -> f64 {
        match entry_side {
            Side::Buy => match self {
                ExitType::Percent(pct) => {
                    precise_round!(origin - (origin * (*pct) / 100.0), 2)
                }
                ExitType::Price(dollars) => precise_round!(origin - dollars, 2),
            },
            Side::Sell => match self {
                ExitType::Percent(pct) => {
                    precise_round!(origin + (origin * (*pct) / 100.0), 2)
                }
                ExitType::Price(dollars) => precise_round!(origin + dollars, 2),
            },
        }
    }

    pub fn calc_take_profit_exit(&self, entry_side: &Side, origin: f64) -> f64 {
        match entry_side {
            Side::Sell => match self {
                ExitType::Percent(pct) => {
                    precise_round!(origin - (origin * (*pct) / 100.0), 2)
                }
                ExitType::Price(dollars) => precise_round!(origin - dollars, 2),
            },
            Side::Buy => match self {
                ExitType::Percent(pct) => {
                    precise_round!(origin + (origin * (*pct) / 100.0), 2)
                }
                ExitType::Price(dollars) => precise_round!(origin + dollars, 2),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct StopLossHandler {
    pub stop_type: ExitType,
}

impl StopLossHandler {
    pub fn new(stop_type: ExitType) -> Self {
        Self { stop_type }
    }

    pub fn build(&self, entry_price: f64, entry_side: Side) -> Result<(Num, Num)> {
        let (stop_price, limit_price) = match entry_side {
            // entry is buy, so stop loss is sell
            Side::Buy => {
                let limit_price = self.stop_type.calc_stop_loss_exit(&entry_side, entry_price);
                // stop price is 75% of the way from entry to limit price
                let stop_price =
                    precise_round!(limit_price + ((limit_price - entry_price).abs() / 4.0), 2);
                (limit_price, stop_price)
            }
            // entry is sell, so stop loss is buy
            Side::Sell => {
                let limit_price = self.stop_type.calc_stop_loss_exit(&entry_side, entry_price);
                // stop price is 75% of the way from entry to limit price
                let stop_price =
                    precise_round!(limit_price - ((limit_price - entry_price).abs() / 4.0), 2);
                (limit_price, stop_price)
            }
        };
        Ok((f64_to_num!(stop_price), f64_to_num!(limit_price)))
    }
}

#[derive(Debug, Clone)]
pub struct TakeProfitHandler {
    pub trail_type: ExitType,
    pub trail_price: Option<Num>,
    pub trail_percent: Option<Num>,
}

impl TakeProfitHandler {
    pub fn new(trail_type: ExitType) -> Self {
        match trail_type {
            ExitType::Percent(pct) => {
                let trail_percent = f64_to_num!(pct);
                Self {
                    trail_type,
                    trail_price: None,
                    trail_percent: Some(trail_percent),
                }
            }
            ExitType::Price(dollars) => {
                let trail_price = f64_to_num!(dollars);
                Self {
                    trail_type,
                    trail_price: Some(trail_price),
                    trail_percent: None,
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum WebSocketEvent {
    OrderUpdate(OrderUpdate),
    Bar(Bar),
}

pub fn status_to_string(status: Status) -> String {
    match status {
        Status::New => "New".to_string(),
        Status::Replaced => "Replaced".to_string(),
        Status::PartiallyFilled => "PartiallyFilled".to_string(),
        Status::Filled => "Filled".to_string(),
        Status::DoneForDay => "DoneForDay".to_string(),
        Status::Canceled => "Canceled".to_string(),
        Status::Expired => "Expired".to_string(),
        Status::Accepted => "Accepted".to_string(),
        Status::PendingNew => "PendingNew".to_string(),
        Status::AcceptedForBidding => "AcceptedForBidding".to_string(),
        Status::PendingCancel => "PendingCancel".to_string(),
        Status::PendingReplace => "PendingReplace".to_string(),
        Status::Stopped => "Stopped".to_string(),
        Status::Rejected => "Rejected".to_string(),
        Status::Suspended => "Suspended".to_string(),
        Status::Calculated => "Calculated".to_string(),
        Status::Held => "Held".to_string(),
        Status::Unknown => "Unknown".to_string(),
    }
}
