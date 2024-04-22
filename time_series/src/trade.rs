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
pub struct Pnl {
  pub quote: f64,
  pub pct: f64,
  pub win_rate: f64,
  pub total_trades: usize,
  pub avg_quote_trade_size: f64,
  pub avg_pct_pnl: f64,
  pub max_pct_drawdown: f64,
  pub quote_data: Vec<Data>,
  pub pct_data: Vec<Data>
}
impl Pnl {
  pub fn print(&self) {
    println!("Return: {}%", self.pct);
    println!("Return: ${}", self.quote);
    println!("Avg Trade Return: ${}", self.avg_pct_pnl);
    println!("Avg Trade Size: ${}", self.avg_quote_trade_size);
    println!("Win Rate: {}%", self.win_rate);
    println!("Max Drawdown: {}%", self.max_pct_drawdown);
    println!("Total Trades: {}", self.total_trades);

  }
}