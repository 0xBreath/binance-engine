#![allow(clippy::unnecessary_cast)]

use std::collections::HashMap;
use crate::{Dataset, Time, trunc};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, Default)]
pub enum Bet {
  #[default]
  Static,
  Percent(f64)
}

#[derive(Debug, Clone, Copy)]
pub enum Source {
  Open,
  High,
  Low,
  Close
}

#[derive(Debug, Clone, PartialEq)]
pub struct SignalInfo {
  pub price: f64,
  pub date: Time,
  pub ticker: String
}

#[derive(Debug, Clone, PartialEq)]
pub enum Signal {
  EnterLong(SignalInfo),
  ExitLong(SignalInfo),
  EnterShort(SignalInfo),
  ExitShort(SignalInfo),
  None
}

impl Signal {
  pub fn print(&self) -> String {
    match self {
      Signal::EnterLong(data) => {
        format!("🟢🟢 Enter Long {}", data.price)
      },
      Signal::ExitLong(data) => {
        format!("🟢 Exit Long {}", data.price)
      },
      Signal::EnterShort(data) => {
        format!("🔴️🔴️ Enter Short {}", data.price)
      },
      Signal::ExitShort(data) => {
        format!("🔴️ Exit Short {}", data.price)
      },
      Signal::None => "No signal".to_string()
    }
  }

  #[allow(dead_code)]
  pub fn price(&self) -> Option<f64> {
    match self {
      Signal::EnterLong(info) => Some(info.price),
      Signal::ExitLong(info) => Some(info.price),
      Signal::EnterShort(info) => Some(info.price),
      Signal::ExitShort(info) => Some(info.price),
      Signal::None => None
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
  EnterLong,
  ExitLong,
  EnterShort,
  ExitShort
}
impl Order {
  pub fn is_entry(&self) -> bool {
    matches!(self, Order::EnterLong | Order::EnterShort)
  }

  pub fn is_exit(&self) -> bool {
    matches!(self, Order::ExitLong | Order::ExitShort)
  }
}

#[derive(Debug, Clone)]
pub struct Trade {
  pub ticker: String,
  pub date: Time,
  pub side: Order,
  /// base asset quantity
  pub quantity: f64,
  pub price: f64
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PerformanceSummary {
  ticker: String,
  pct_roi: f64,
  quote_roi: f64,
  total_trades: usize,
  win_rate: f64,
  avg_trade_size: f64,
  avg_trade: f64,
  avg_winning_trade: f64,
  avg_losing_trade: f64,
  best_trade: f64,
  worst_trade: f64,
  max_drawdown: f64
}

#[derive(Debug, Clone)]
pub struct Summary {
  pub cum_quote: HashMap<String, Dataset<i64, f64>>,
  pub cum_pct: HashMap<String, Dataset<i64, f64>>,
  pub pct_per_trade: HashMap<String, Dataset<i64, f64>>,
  pub trades: HashMap<String, Vec<Trade>>,
}
impl Summary {
  pub fn print(&self, ticker: &str) {
    println!("==== {} Backtest Summary ====", ticker);
    println!("Return: {}%", self.pct_roi(ticker));
    println!("Return: ${}", self.quote_roi(ticker));
    println!("Total Trades: {}", self.total_trades(ticker));
    println!("Win Rate: {}%", self.win_rate(ticker));
    println!("Avg Trade Size: ${}", self.avg_trade_size(ticker).unwrap());
    println!("Avg Trade: {}%", self.avg_trade(ticker));
    println!("Avg Winning Trade: {}%", self.avg_winning_trade(ticker));
    println!("Avg Losing Trade: {}%", self.avg_losing_trade(ticker));
    println!("Best Trade: {}%", self.best_trade(ticker));
    println!("Worst Trade: {}%", self.worst_trade(ticker));
    println!("Max Drawdown: {}%", self.max_drawdown(ticker));
  }
  
  pub fn cum_quote(&self, ticker: &str) -> anyhow::Result<&Dataset<i64, f64>> {
    self.cum_quote.get(ticker).ok_or(anyhow::anyhow!("No cum quote for ticker"))
  }

  pub fn cum_pct(&self, ticker: &str) -> anyhow::Result<&Dataset<i64, f64>> {
    self.cum_pct.get(ticker).ok_or(anyhow::anyhow!("No cum pct for ticker"))
  }

  pub fn pct_per_trade(&self, ticker: &str) -> anyhow::Result<&Dataset<i64, f64>> {
    self.pct_per_trade.get(ticker).ok_or(anyhow::anyhow!("No pct per trade for ticker"))
  }

  pub fn trades(&self, ticker: &str) -> anyhow::Result<&Vec<Trade>> {
    self.trades.get(ticker).ok_or(anyhow::anyhow!("No trades for ticker"))
  }

  pub fn summarize(&self, ticker: &str) -> anyhow::Result<PerformanceSummary> {
    Ok(PerformanceSummary {
      ticker: ticker.to_string(),
      pct_roi: self.pct_roi(ticker),
      quote_roi: self.quote_roi(ticker),
      total_trades: self.total_trades(ticker),
      win_rate: self.win_rate(ticker),
      avg_trade_size: self.avg_trade_size(ticker)?,
      avg_trade: self.avg_trade(ticker),
      avg_winning_trade: self.avg_winning_trade(ticker),
      avg_losing_trade: self.avg_losing_trade(ticker),
      best_trade: self.best_trade(ticker),
      worst_trade: self.worst_trade(ticker),
      max_drawdown: self.max_drawdown(ticker)
    })
  }

  pub fn total_trades(&self, ticker: &str) -> usize {
    self.cum_pct.get(ticker).unwrap().data().len()
  }

  pub fn avg_trade_size(&self, ticker: &str) -> anyhow::Result<f64> {
    let trades = self.trades.get(ticker).ok_or(anyhow::anyhow!("No trades for ticker"))?;
    let avg = trades.iter().map(|t| {
      t.price * t.quantity
    }).sum::<f64>() / trades.len() as f64;
    Ok(trunc!(avg, 2))
  }

  pub fn quote_roi(&self, ticker: &str) -> f64 {
    let ending_quote_roi = self.cum_quote.get(ticker).unwrap().data().last().unwrap().y;
    trunc!(ending_quote_roi, 3)
  }

  pub fn pct_roi(&self, ticker: &str) -> f64 {
    let ending_pct_roi = self.cum_pct.get(ticker).unwrap().data().last().unwrap().y;
    trunc!(ending_pct_roi, 3)
  }

  pub fn max_drawdown(&self, ticker: &str) -> f64 {
    let mut max_dd = 0.0;
    let mut peak = self.cum_pct.get(ticker).unwrap().data().first().unwrap().y;

    for point in self.cum_pct.get(ticker).unwrap().data().iter() {
      if point.y > peak {
        peak = point.y;
      } else {
        // 1000 + 14% = 1140
        // 1000 - 35% = 650
        // max drawdown = -35 - 14 = -49
        // 650 - 1140 / 1140 = -0.43
        let y = 1.0 + point.y / 100.0; // 14% = 1.14, -35% = 0.65
        let p = 1.0 + peak / 100.0;
        let dd = (y - p) / p * 100.0;
        // let dd = point.y - peak;
        if dd < max_dd {
          max_dd = dd;
        }
      }
    }
    trunc!(max_dd, 3)
  }

  pub fn avg_trade(&self, ticker: &str) -> f64 {
    let len = self.pct_per_trade.get(ticker).unwrap().data().len();
    let avg_trade = self.pct_per_trade
      .get(ticker)
      .unwrap()
      .data()
      .iter()
      .map(|d| d.y).sum::<f64>() / len as f64;
    trunc!(avg_trade, 3)
  }

  pub fn avg_winning_trade(&self, ticker: &str) -> f64 {
    let len = self.pct_per_trade.get(ticker).unwrap().data().iter().filter(|d| d.y > 0.0).count();
    let avg_winning_trade = self.pct_per_trade
      .get(ticker)
      .unwrap()
      .data()
      .iter()
      .filter(|d| d.y > 0.0)
      .map(|d| d.y).sum::<f64>() / len as f64;
    trunc!(avg_winning_trade, 3)
  }

  pub fn avg_losing_trade(&self, ticker: &str) -> f64 {
    let len = self.pct_per_trade.get(ticker).unwrap().data().iter().filter(|d| d.y < 0.0).count();
    let avg_losing_trade = self.pct_per_trade
      .get(ticker)
      .unwrap()
      .data()
      .iter()
      .filter(|d| d.y < 0.0).map(|d| d.y).sum::<f64>() / len as f64;
    trunc!(avg_losing_trade, 3)
  }

  pub fn best_trade(&self, ticker: &str) -> f64 {
    let best_trade = self.pct_per_trade
      .get(ticker)
      .unwrap()
      .data()
      .iter()
      .map(|d| d.y)
      .max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();
    trunc!(best_trade, 3)
  }

  pub fn worst_trade(&self, ticker: &str) -> f64 {
    let worst_trade = self.pct_per_trade
      .get(ticker)
      .unwrap()
      .data()
      .iter()
      .map(|d| d.y)
      .min_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();
    trunc!(worst_trade, 3)
  }

  pub fn win_rate(&self, ticker: &str) -> f64 {
    let len = self.pct_per_trade.get(ticker).unwrap().data().len();
    let win_rate = self.pct_per_trade
      .get(ticker)
      .unwrap()
      .data()
      .iter()
      .filter(|d| d.y > 0.0).count() as f64 / len as f64 * 100.0;
    trunc!(win_rate, 3)
  }
}