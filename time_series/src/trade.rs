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
        format!("🟢 Long {}", data.0)
      },
      Signal::Short(data) => {
        format!("🔴️ Short {}", data.0)
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
  pub cum_quote: Dataset,
  pub cum_pct: Dataset,
  pub pct_per_trade: Dataset
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
    let mut max_drawdown = 0.0;
    let mut peak = self.cum_pct.data().first().unwrap().y;
    for d in self.cum_pct.data().iter() {
      if d.y > peak {
        peak = d.y;
      }
      let drawdown = d.y - peak;
      if drawdown < max_drawdown {
        max_drawdown = drawdown;
      }
    }
    // convert the quote ($) to pct relative to the peak
    // -200 / 300 = -0.6666666666666666 * 100 = -66.67%
    println!("drawdown: {}, peak: {}", max_drawdown, peak);
    let max_drawdown = max_drawdown / peak * 100.0;
    trunc!(max_drawdown, 2)
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