#![allow(dead_code)]

use std::str::FromStr;
use crate::{BinanceTrade, Timestamp};
use crate::model::*;
use serde::{Serialize, Deserialize};
use time_series::{Time, Trade};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeInfo {
  pub client_order_id: String,
  pub order_type: OrderType,
  pub status: OrderStatus,
  pub event_time: i64,
  pub quantity: f64,
  pub price: f64,
  pub side: Side,
}
impl Timestamp for TradeInfo {
  fn timestamp(&self) -> i64 {
    self.event_time
  }
}

impl TryFrom<&HistoricalOrder> for TradeInfo {
  type Error = anyhow::Error;

  fn try_from(historical_order: &HistoricalOrder) -> Result<Self, Self::Error> {
    Ok(Self {
      client_order_id: historical_order.client_order_id.clone(),
      order_type: OrderType::from_str(historical_order._type.as_str())?,
      status: OrderStatus::from_str(&historical_order.status)?,
      event_time: historical_order.update_time,
      quantity: historical_order.executed_qty.parse::<f64>()?,
      price: historical_order.price.parse::<f64>()?,
      side: Side::from_str(&historical_order.side)?,
    })
  }
}

impl TryFrom<&OrderTradeEvent> for TradeInfo {
  type Error = anyhow::Error;

  fn try_from(order_trade_event: &OrderTradeEvent) -> Result<Self, Self::Error> {
    Ok(Self {
      client_order_id: order_trade_event.new_client_order_id.clone(),
      order_type: OrderType::from_str(order_trade_event.order_type.as_str())?,
      status: OrderStatus::from_str(&order_trade_event.order_status)?,
      event_time: order_trade_event.event_time as i64,
      quantity: order_trade_event.qty.parse::<f64>()?,
      price: order_trade_event.price.parse::<f64>()?,
      side: Side::from_str(&order_trade_event.side)?,
    })
  }
}

impl TradeInfo {
  pub fn to_trade(&self, ticker: String) -> anyhow::Result<Trade> {
    Ok(Trade {
      ticker,
      date: Time::from_unix_ms(self.event_time),
      // todo: handle short selling
      side: match self.side {
        Side::Long => time_series::Order::EnterLong,
        Side::Short => time_series::Order::ExitLong,
      },
      quantity: self.quantity,
      price: self.price
    })
  }
}

pub struct OrderBuilder {
  pub entry: BinanceTrade,
  pub stop_loss: Option<BinanceTrade>
}

#[derive(Debug, Clone)]
pub enum OrderState {
  Pending(BinanceTrade),
  Active(TradeInfo),
}
impl OrderState {
  pub fn client_order_id(&self) -> String {
    match &self {
      OrderState::Pending(order) => order.client_order_id.clone(),
      OrderState::Active(trade_info) => trade_info.client_order_id.clone(),
    }
  }
}
impl Timestamp for OrderState {
  fn timestamp(&self) -> i64 {
    match &self {
      OrderState::Pending(order) => order.timestamp,
      OrderState::Active(trade_info) => trade_info.event_time,
    }
  }
}

#[derive(Debug, Clone, Default)]
pub struct ActiveOrder {
  pub entry: Option<OrderState>,
  pub stop_loss: Option<OrderState>,
  pub stop_loss_placed: bool
}

impl ActiveOrder {
  pub fn new() -> Self {
    Self::default()
  }
  
  pub fn client_order_id_prefix(client_order_id: &str) -> String {
    client_order_id.split('-').next().unwrap().to_string()
  }

  pub fn client_order_id_suffix(client_order_id: &str) -> String {
    client_order_id.split('-').last().unwrap().to_string()
  }
  
  pub fn add_entry(&mut self, order: BinanceTrade) {
    self.entry = Some(OrderState::Pending(order));
  }
  
  pub fn add_stop_loss(&mut self, order: BinanceTrade) {
    self.stop_loss = Some(OrderState::Pending(order));
  }

  pub fn reset(&mut self) {
    self.entry = None;
    self.stop_loss = None;
    self.stop_loss_placed = false;
  }
}