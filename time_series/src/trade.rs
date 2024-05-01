#![allow(clippy::unnecessary_cast)]

use crate::{Dataset, Time, trunc};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy)]
pub enum Bet {
  Static(f64),
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

#[derive(Debug, Serialize, Deserialize)]
pub struct PerformanceSummary {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
  /// Average quote amount per trade
  pub avg_trade_size: f64,
  pub cum_quote: Dataset<f64>,
  pub cum_pct: Dataset<f64>,
  pub pct_per_trade: Dataset<f64>
}
impl Summary {
  pub fn print(&self) {
    println!("Return: {}%", self.pct_roi());
    println!("Return: ${}", self.quote_roi());
    println!("Total Trades: {}", self.total_trades());
    println!("Win Rate: {}%", self.win_rate());
    println!("Avg Trade Size: ${}", self.avg_trade_size);
    println!("Avg Trade: {}%", self.avg_trade());
    println!("Avg Winning Trade: {}%", self.avg_winning_trade());
    println!("Avg Losing Trade: {}%", self.avg_losing_trade());
    println!("Best Trade: {}%", self.best_trade());
    println!("Worst Trade: {}%", self.worst_trade());
    println!("Max Drawdown: {}%", self.max_drawdown());
  }

  pub fn summarize(&self) -> PerformanceSummary {
    #[derive(Debug, Serialize, Deserialize)]
    struct Summarize {
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
    PerformanceSummary {
      pct_roi: self.pct_roi(),
      quote_roi: self.quote_roi(),
      total_trades: self.total_trades(),
      win_rate: self.win_rate(),
      avg_trade_size: self.avg_trade_size,
      avg_trade: self.avg_trade(),
      avg_winning_trade: self.avg_winning_trade(),
      avg_losing_trade: self.avg_losing_trade(),
      best_trade: self.best_trade(),
      worst_trade: self.worst_trade(),
      max_drawdown: self.max_drawdown()
    }
  }

  pub fn total_trades(&self) -> usize {
    self.cum_pct.data().len()
  }

  pub fn quote_roi(&self) -> f64 {
    let ending_quote_roi = self.cum_quote.data().last().unwrap().y;
    trunc!(ending_quote_roi, 2)
  }

  pub fn pct_roi(&self) -> f64 {
    let ending_pct_roi = self.cum_pct.data().last().unwrap().y;
    trunc!(ending_pct_roi, 2)
  }

  pub fn max_drawdown(&self) -> f64 {
    let mut max_dd = 0.0;
    let mut peak = self.cum_quote.data().first().unwrap().y;

    for point in self.cum_quote.data().iter() {
      if point.y > peak {
        peak = point.y;
      } else {
        // -200 - 1400 = = -1600 / 1400 * 100 = -114.29%
        let dd = ((point.y - peak) / peak * 100.0).max(-100.0);
        if dd < max_dd {
          max_dd = dd;
        }
      }
    }
    trunc!(max_dd, 2)
  }

  pub fn avg_trade(&self) -> f64 {
    let avg_trade = self.pct_per_trade.data().iter().map(|d| d.y).sum::<f64>() / self.pct_per_trade.data().len() as f64;
    trunc!(avg_trade, 2)
  }

  pub fn avg_winning_trade(&self) -> f64 {
    let avg_winning_trade = self.pct_per_trade.data().iter().filter(|d| d.y > 0.0).map(|d| d.y).sum::<f64>() / self.pct_per_trade.data().iter().filter(|d| d.y > 0.0).count() as f64;
    trunc!(avg_winning_trade, 2)
  }

  pub fn avg_losing_trade(&self) -> f64 {
    let avg_losing_trade = self.pct_per_trade.data().iter().filter(|d| d.y < 0.0).map(|d| d.y).sum::<f64>() / self.pct_per_trade.data().iter().filter(|d| d.y < 0.0).count() as f64;
    trunc!(avg_losing_trade, 2)
  }

  pub fn best_trade(&self) -> f64 {
    let best_trade = self.pct_per_trade.data().iter().map(|d| d.y).max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();
    trunc!(best_trade, 2)
  }

  pub fn worst_trade(&self) -> f64 {
    let worst_trade = self.pct_per_trade.data().iter().map(|d| d.y).min_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();
    trunc!(worst_trade, 2)
  }

  pub fn win_rate(&self) -> f64 {
    let win_rate = self.pct_per_trade.data().iter().filter(|d| d.y > 0.0).count() as f64 / self.pct_per_trade.data().len() as f64 * 100.0;
    trunc!(win_rate, 2)
  }
}