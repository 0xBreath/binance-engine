use std::collections::VecDeque;
use std::str::FromStr;
use time_series::{Candle, Time};
use crate::BinanceTrade;
use crate::model::*;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeInfo {
  pub client_order_id: String,
  pub order_id: u64,
  pub order_type: OrderType,
  pub status: OrderStatus,
  pub event_time: i64,
  pub quantity: f64,
  pub price: f64,
  pub side: Side,
}

impl TradeInfo {
  #[allow(dead_code)]
  pub fn from_historical_order(historical_order: &HistoricalOrder) -> anyhow::Result<Self> {
    Ok(Self {
      client_order_id: historical_order.client_order_id.clone(),
      order_id: historical_order.order_id,
      order_type: OrderType::from_str(historical_order._type.as_str())?,
      status: OrderStatus::from_str(&historical_order.status)?,
      event_time: historical_order.update_time,
      quantity: historical_order.executed_qty.parse::<f64>()?,
      price: historical_order.price.parse::<f64>()?,
      side: Side::from_str(&historical_order.side)?,
    })
  }

  pub fn from_order_trade_event(order_trade_event: &OrderTradeEvent) -> anyhow::Result<Self> {
    let order_type = OrderType::from_str(order_trade_event.order_type.as_str())?;
    let status = OrderStatus::from_str(&order_trade_event.order_status)?;
    Ok(Self {
      client_order_id: order_trade_event.new_client_order_id.clone(),
      order_id: order_trade_event.order_id,
      order_type,
      status,
      event_time: order_trade_event.event_time as i64,
      quantity: order_trade_event.qty.parse::<f64>()?,
      price: order_trade_event.price.parse::<f64>()?,
      side: Side::from_str(&order_trade_event.side)?,
    })
  }
}

pub fn kline_to_candle(kline_event: &KlineEvent) -> anyhow::Result<Candle> {
  let date = Time::from_unix_msec(kline_event.event_time as i64);
  Ok(Candle {
    date,
    open: kline_event.kline.open.parse::<f64>()?,
    high: kline_event.kline.high.parse::<f64>()?,
    low: kline_event.kline.low.parse::<f64>()?,
    close: kline_event.kline.close.parse::<f64>()?,
    volume: None,
  })
}

pub struct OrderBuilder {
  pub entry: BinanceTrade,
}

#[derive(Debug, Clone)]
pub enum PendingOrActiveOrder {
  Pending(BinanceTrade),
  Active(TradeInfo),
}

#[derive(Debug, Clone)]
pub struct ActiveOrder {
  pub entry: Option<PendingOrActiveOrder>,
}

impl ActiveOrder {
  #[allow(clippy::too_many_arguments)]
  pub fn new() -> Self {
    Self {
      entry: None,
    }
  }

  #[allow(dead_code)]
  pub fn client_order_id_prefix(client_order_id: &str) -> String {
    client_order_id.split('-').next().unwrap().to_string()
  }

  pub fn client_order_id_suffix(client_order_id: &str) -> String {
    client_order_id.split('-').last().unwrap().to_string()
  }

  #[allow(dead_code)]
  pub fn add_entry(&mut self, entry: BinanceTrade) {
    self.entry = Some(PendingOrActiveOrder::Pending(entry));
  }

  pub fn reset(&mut self) {
    self.entry = None;
  }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub enum Source {
  Open,
  High,
  Low,
  Close
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Signal {
  Long((f64, Time)),
  Short((f64, Time)),
  None
}
impl Signal {
  pub fn print(&self) -> String {
    match self {
      Signal::Long(data) => {
        format!("🟢 Long signal {}", data.0)
      },
      Signal::Short(data) => {
        format!("🔴️ Short signal {}", data.0)
      },
      Signal::None => "No signal".to_string()
    }
  }

  #[allow(dead_code)]
  pub fn price(&self) -> Option<f64> {
    match self {
      Signal::Long((price, _)) => Some(*price),
      Signal::Short((price, _)) => Some(*price),
      Signal::None => None
    }
  }
}

#[derive(Debug, Clone)]
pub struct RollingCandles {
  pub vec: VecDeque<Candle>,
  pub capacity: usize,
}
impl RollingCandles {
  pub fn new(capacity: usize) -> Self {
    Self {
      vec: VecDeque::with_capacity(capacity),
      capacity,
    }
  }

  pub fn push(&mut self, candle: Candle) {
    if self.vec.len() == self.capacity {
      self.vec.pop_back();
    }
    self.vec.push_front(candle);
  }
}

#[derive(Debug, Clone)]
pub struct Data {
  pub x: i64,
  pub y: f64,
}
