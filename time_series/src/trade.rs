use crate::{Data, Time};
use serde::{Serialize, Deserialize};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
  pub roi: f64,
  pub pnl: f64,
  pub win_rate: f64,
  pub total_trades: usize,
  /// Average quote amount per trade
  pub avg_trade_size: f64,
  /// Average quote earned per trade
  pub avg_trade_roi: f64,
  pub avg_trade_pnl: f64,
  pub max_pct_drawdown: f64,
  pub roi_data: Vec<Data>
}
impl Summary {
  pub fn print(&self) {
    println!("Return: {}%", self.pnl);
    println!("Return: ${}", self.roi);
    println!("Avg Trade Return: ${}", self.avg_trade_pnl);
    println!("Avg Trade Size: ${}", self.avg_trade_size);
    println!("Win Rate: {}%", self.win_rate);
    println!("Max Drawdown: {}%", self.max_pct_drawdown);
    println!("Total Trades: {}", self.total_trades);

  }
}