use crate::{Candle, Time};
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone)]
pub enum ReversalType {
    High,
    Low,
}
impl ReversalType {
    pub fn as_string(&self) -> String {
        match self {
            ReversalType::High => "High".to_string(),
            ReversalType::Low => "Low".to_string(),
        }
    }
}
impl PartialEq for ReversalType {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (ReversalType::High, ReversalType::High) | (ReversalType::Low, ReversalType::Low)
        )
    }
}

#[derive(Debug, Clone)]
pub struct Reversal {
    pub candle: Candle,
    pub reversal_type: ReversalType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
}
impl Direction {
    pub fn as_string(&self) -> &str {
        match self {
            Direction::Up => "Up",
            Direction::Down => "Down",
        }
    }
}
impl Display for Direction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Direction::Up => write!(f, "Up"),
            Direction::Down => write!(f, "Down"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Order {
    Long,
    Short,
}

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum TrailingStopType {
    Percent,
    Pips,
}

#[derive(Debug, Clone)]
pub struct Trade {
    /// Time of trade entry
    pub entry_date: Time,
    /// Time of trade exit
    pub exit_date: Option<Time>,
    /// Long or Short
    pub order: Order,
    /// Amount of base asset
    pub contracts: f64,
    /// Entry price
    pub entry_price: f64,
    /// Quote asset amount to risk
    pub capital: f64,
    /// Exit price
    pub exit_price: Option<f64>,
    /// Percent profit or loss relative to capital
    pub pnl: Option<f64>,
    /// Trailing stop
    pub trailing_stop: Option<f64>,
    /// Stop loss
    pub stop_loss: Option<f64>,
}
impl Trade {
    pub fn new(
        entry_date: Time,
        order: Order,
        contracts: f64,
        entry_price: f64,
        capital: f64,
        trailing_stop: Option<f64>,
        stop_loss: Option<f64>,
    ) -> Self {
        Self {
            entry_date,
            exit_date: None,
            order,
            contracts,
            entry_price,
            capital,
            exit_price: None,
            pnl: None,
            trailing_stop,
            stop_loss,
        }
    }

    pub fn trade_quantity(capital: f64, price: f64) -> f64 {
        let quantity = capital / price;
        (quantity * 1000000.0).round() / 1000000.0
    }

    pub fn exit(&mut self, exit_date: Time, exit_price: f64) {
        self.exit_date = Some(exit_date);
        self.exit_price = Some(exit_price);
        let pnl = self.pnl();
        self.pnl = Some(pnl);
    }

    pub fn quote_asset_pnl(&self) -> f64 {
        let exit_price = self.exit_price.unwrap();
        let entry_price = self.entry_price;
        let contracts = self.contracts;
        match self.order {
            Order::Long => (exit_price - entry_price) * contracts,
            Order::Short => (entry_price - exit_price) * contracts,
        }
    }

    pub fn pnl(&self) -> f64 {
        let exit_price = self.exit_price.unwrap();
        let entry_price = self.entry_price;
        let contracts = self.contracts;
        let pnl = match self.order {
            Order::Long => (exit_price - entry_price) * contracts,
            Order::Short => (entry_price - exit_price) * contracts,
        };
        pnl / self.capital * 100.0
    }

    pub fn calc_stop_loss(order: Order, price: f64, stop_loss_pct: f64) -> f64 {
        match order {
            Order::Long => price * (1.0 - stop_loss_pct),
            Order::Short => price * (1.0 + stop_loss_pct),
        }
    }

    /// Find value of one pip for a given candle price.
    /// One pip equals the smallest decimal place of ticker.
    fn find_pip_value(price: f64) -> f64 {
        let mut decimals = 0;
        let mut price = price;
        while price.fract() != 0.0 {
            price *= 10.0;
            decimals += 1;
        }
        let power = 10.0_f64.powi(decimals);
        1.0 / power
    }

    pub fn calc_trailing_stop(
        order: Order,
        price: f64,
        trailing_stop_type: TrailingStopType,
        trailing_stop: f64,
    ) -> f64 {
        match trailing_stop_type {
            TrailingStopType::Percent => match order {
                Order::Long => price * (1.0 - trailing_stop),
                Order::Short => price * (1.0 + trailing_stop),
            },
            TrailingStopType::Pips => {
                let pip_value = Self::find_pip_value(price);
                match order {
                    Order::Long => price - trailing_stop * pip_value,
                    Order::Short => price + trailing_stop * pip_value,
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Backtest {
    pub trades: Vec<Trade>,
    pub pnl: Option<f64>,
    pub capital: f64,
    pub start_date: Option<Time>,
    pub end_date: Option<Time>,
    pub avg_trade_pnl: Option<f64>,
    pub avg_win_trade_pnl: Option<f64>,
    pub avg_loss_trade_pnl: Option<f64>,
}
impl Backtest {
    pub fn new(capital: f64) -> Self {
        Self {
            trades: vec![],
            pnl: None,
            capital,
            start_date: None,
            end_date: None,
            avg_trade_pnl: None,
            avg_win_trade_pnl: None,
            avg_loss_trade_pnl: None,
        }
    }

    pub fn add_trade(&mut self, trade: Trade) {
        self.trades.push(trade);
        self.pnl = self.pnl();
    }

    pub fn pnl(&self) -> Option<f64> {
        let mut pnl = 0.0;
        for trade in &self.trades {
            if let Some(trade_pnl) = trade.pnl {
                pnl += trade_pnl;
            } else {
                println!(
                    "No trade PNL, entry {}, exit {}",
                    trade.entry_price,
                    trade.exit_price.unwrap()
                );
            }
        }
        if pnl == 0.0 {
            None
        } else {
            Some(pnl)
        }
    }

    pub fn quote_asset_pnl(&self) -> f64 {
        let mut pnl = 0.0;
        for trade in &self.trades {
            pnl += trade.quote_asset_pnl();
        }
        pnl
    }

    pub fn avg_trade_pnl(&self) -> Option<f64> {
        if let Some(pnl) = self.pnl {
            let trades = self.trades.len();
            if trades == 0 {
                None
            } else {
                Some(pnl / trades as f64)
            }
        } else {
            None
        }
    }

    pub fn avg_win_trade_pnl(&self) -> Option<f64> {
        let mut pnl = 0.0;
        let mut trades = 0;
        for trade in &self.trades {
            if let Some(trade_pnl) = trade.pnl {
                if trade_pnl > 0.0 {
                    pnl += trade_pnl;
                    trades += 1;
                }
            }
        }
        if trades == 0 {
            None
        } else {
            Some(pnl / trades as f64)
        }
    }

    pub fn avg_loss_trade_pnl(&self) -> Option<f64> {
        let mut pnl = 0.0;
        let mut trades = 0;
        for trade in &self.trades {
            if let Some(trade_pnl) = trade.pnl {
                if trade_pnl < 0.0 {
                    pnl += trade_pnl;
                    trades += 1;
                }
            }
        }
        if trades == 0 {
            None
        } else {
            Some(pnl / trades as f64)
        }
    }

    pub fn num_trades(&self) -> usize {
        self.trades.len()
    }

    pub fn num_win_trades(&self) -> usize {
        let mut trades = 0;
        for trade in &self.trades {
            if let Some(trade_pnl) = trade.pnl {
                if trade_pnl > 0.0 {
                    trades += 1;
                }
            }
        }
        trades
    }

    pub fn num_loss_trades(&self) -> usize {
        let mut trades = 0;
        for trade in &self.trades {
            if let Some(trade_pnl) = trade.pnl {
                if trade_pnl < 0.0 {
                    trades += 1;
                }
            }
        }
        trades
    }

    pub fn summarize(&mut self) {
        if self.trades.is_empty() {
            return;
        }
        self.start_date = Some(self.trades.first().unwrap().entry_date);
        self.end_date = Some(self.trades.last().unwrap().entry_date);
        self.avg_trade_pnl = self.avg_trade_pnl();
        self.avg_win_trade_pnl = self.avg_win_trade_pnl();
        self.avg_loss_trade_pnl = self.avg_loss_trade_pnl();
    }
}
