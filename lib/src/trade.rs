use std::str::FromStr;
use crate::{BinanceTrade};
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

pub struct OrderBuilder {
  pub entry: BinanceTrade,
}

#[derive(Debug, Clone)]
pub enum PendingOrActiveOrder {
  Pending(BinanceTrade),
  Active(TradeInfo),
}

#[derive(Debug, Clone, Default)]
pub struct ActiveOrder {
  pub entry: Option<PendingOrActiveOrder>,
}

impl ActiveOrder {
  pub fn new() -> Self {
    Self::default()
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