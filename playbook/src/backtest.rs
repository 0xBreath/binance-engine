#![allow(dead_code)]
#![allow(clippy::unnecessary_cast)]

use std::collections::HashMap;
use time_series::{Bet, Candle, Data, DataCache, Dataset, Order, Signal, Summary, Time, Trade, trunc};
use std::marker::PhantomData;
use lib::{Account};
use crate::Strategy;

#[derive(Debug, Clone, Default)]
pub struct EmptyStrategy;
impl Strategy<f64> for EmptyStrategy {
  /// Receives new candle and returns a signal (long, short, or do nothing).
  fn process_candle(&mut self, _candle: Candle, _ticker: Option<String>) -> anyhow::Result<Vec<Signal>> {
    Ok(vec![])
  }
  /// Appends a candle to the candle cache
  fn push_candle(&mut self, _candle: Candle, _ticker: Option<String>) {}
  /// Returns a reference to the candle cache
  fn cache(&self, _ticker: Option<String>) -> Option<&DataCache<f64>> {
    None
  }

  fn stop_loss_pct(&self) -> Option<f64> { None }
}
impl EmptyStrategy {
  pub fn new() -> Self {
    Self
  }
}

#[derive(Debug, Clone)]
pub struct Backtest<T, S: Strategy<T>> {
  pub strategy: S,
  pub capital: f64,
  /// Fee in percentage
  pub fee: f64,
  /// If compounded, assumes trading profits are 100% reinvested.
  /// If not compounded, assumed trading with initial capital (e.g. $1000 every trade) and not reinvesting profits.
  pub bet: Bet,
  pub leverage: u8,
  /// False if spot trading, true if margin trading which allows short selling
  pub short_selling: bool,
  pub candles: HashMap<String, Vec<Candle>>,
  pub trades: HashMap<String, Vec<Trade>>,
  pub signals: HashMap<String, Vec<Signal>>,
  _data: PhantomData<T>
}

impl Default for Backtest<f64, EmptyStrategy> {
  fn default() -> Self {
    Self {
      strategy: EmptyStrategy::new(),
      capital: 1000.0,
      fee: 0.0,
      bet: Bet::Static,
      leverage: 1,
      short_selling: false,
      candles: HashMap::new(),
      trades: HashMap::new(),
      signals: HashMap::new(),
      _data: PhantomData
    }
  }
}

impl<T, S: Strategy<T>> Backtest<T, S> {
  pub fn new(strategy: S, capital: f64, fee: f64, bet: Bet, leverage: u8, short_selling: bool) -> Self {
    Self {
      strategy,
      capital,
      fee,
      bet,
      leverage,
      short_selling,
      candles: HashMap::new(),
      trades: HashMap::new(),
      signals: HashMap::new(),
      _data: PhantomData
    }
  }

  pub async fn klines(account: &Account, start_time: Option<Time>, end_time: Option<Time>) -> anyhow::Result<Vec<Candle>> {
    let days_back = match (start_time, end_time) {
      (Some(start), Some(end)) => {
        start.diff_days(&end)?
      },
      (Some(start), None) => {
        start.diff_days(&Time::now())?
      },
      _ => 30
    };
    println!("days back: {}", days_back);
    let mut klines = account.kline_history(days_back).await?;
    klines.sort_by(|a, b| a.open_time.cmp(&b.open_time));

    let mut candles = vec![];
    for kline in klines.into_iter() {
      candles.push(kline.to_candle());
    }
    // only take candles greater than a timestamp
    candles.retain(|candle| {
      match (start_time, end_time) {
        (Some(start), Some(end)) => {
          candle.date.to_unix_ms() > start.to_unix_ms() && candle.date.to_unix_ms() < end.to_unix_ms()
        },
        (Some(start), None) => {
          candle.date.to_unix_ms() > start.to_unix_ms()
        },
        (None, Some(end)) => {
          candle.date.to_unix_ms() < end.to_unix_ms()
        },
        (None, None) => true
      }
    });
    Ok(candles)
  }

  pub fn add_candle(&mut self, candle: Candle, ticker: String) {
    let mut candles = self.candles.get(&ticker).unwrap_or(&vec![]).clone();
    candles.push(candle);
    self.candles.insert(ticker, candles);
  }

  pub fn add_trade(&mut self, trade: Trade, ticker: String) {
    let mut trades = self.trades.get(&ticker).unwrap_or(&vec![]).clone();
    trades.push(trade);
    self.trades.insert(ticker, trades);
  }

  pub fn add_signal(&mut self, signal: Signal, ticker: String) {
    let mut signals = self.signals.get(&ticker).unwrap_or(&vec![]).clone();
    signals.push(signal);
    self.signals.insert(ticker, signals);
  }

  pub fn reset(&mut self) {
    self.trades.clear();
    self.signals.clear();
  }

  pub fn buy_and_hold(
    &mut self,
  ) -> anyhow::Result<HashMap<String, Vec<Data<i64, f64>>>> {
    let mut all_data = HashMap::new();
    let candles = self.candles.clone();
    for (ticker, candles) in candles {
      let first = candles.first().unwrap();
      let mut data = vec![];

      for candles in candles.windows(2) {
        let entry = candles[0];
        let exit = candles[1];
        let pct_pnl = ((exit.close - first.close) / first.close) * 100.0;

        data.push(Data {
          x: entry.date.to_unix_ms(),
          y: pct_pnl
        });
      }
      all_data.insert(ticker, data);
    }
    Ok(all_data)
  }

  pub fn backtest(
    &mut self,
  ) -> anyhow::Result<Summary> {
    let candles = self.candles.clone();
    
    let static_capital = self.capital * self.leverage as f64;
    let initial_capital = self.capital;

    let mut cum_capital: HashMap<String, f64> = HashMap::new();
    let mut quote: HashMap<String, f64> = HashMap::new();
    let mut cum_pct: HashMap<String, Vec<Data<i64, f64>>> = HashMap::new();
    let mut cum_quote: HashMap<String, Vec<Data<i64, f64>>> = HashMap::new();
    let mut pct_per_trade:  HashMap<String, Vec<Data<i64, f64>>> = HashMap::new();

    if let Some((_, first_series)) = candles.iter().next() {
      let length = first_series.len();

      let mut active_trades: HashMap<String, Option<Trade>> = HashMap::new();
      for (ticker, _) in candles.iter() {
        // populate active trades with None values for each ticker so getter doesn't panic
        active_trades.insert(ticker.clone(), None);
        // populate with empty vec for each ticker so getter doesn't panic
        self.trades.insert(ticker.clone(), vec![]);
        // populate all tickers with starting values
        cum_capital.insert(ticker.clone(), self.capital * self.leverage as f64);
        quote.insert(ticker.clone(), 0.0);
        cum_pct.insert(ticker.clone(), vec![]);
        cum_quote.insert(ticker.clone(), vec![]);
        pct_per_trade.insert(ticker.clone(), vec![]);
      }

      // Iterate over the index of each series
      for i in 0..length {
        // Access the i-th element of each vector to simulate getting price update
        // for every ticker at roughly the same time
        for (ticker, candles) in candles.iter() {
          let candle = candles[i];

          // check if stop loss is hit
          if let (Some(entry), Some(stop_loss_pct)) = (active_trades.get(ticker).unwrap(), self.strategy.stop_loss_pct()) {
            match entry.side {
              Order::EnterLong => {
                let pct_diff = (candle.low - entry.price) / entry.price * 100.0;
                if pct_diff < stop_loss_pct * -1.0 {
                  let price_at_stop_loss = entry.price * (1.0 - stop_loss_pct / 100.0);
                  // longs are stopped out by the low
                  let pct_pnl = (price_at_stop_loss - entry.price) / entry.price * 100.0;
                  let position_size = match self.bet {
                    Bet::Static => static_capital,
                    Bet::Percent(pct) => *cum_capital.get(ticker).unwrap() * pct / 100.0
                  };

                  // add entry trade with updated quantity
                  let quantity = position_size / entry.price;
                  let updated_entry = Trade {
                    ticker: ticker.clone(),
                    date: entry.date,
                    side: entry.side,
                    quantity,
                    price: entry.price
                  };
                  self.add_trade(updated_entry, ticker.clone());

                  // fee on trade entry capital
                  let entry_fee = position_size.abs() * (self.fee / 100.0);
                  let cum_capital = cum_capital.get_mut(ticker).unwrap();
                  *cum_capital -= entry_fee;
                  
                  // fee on profit made
                  let mut quote_pnl = pct_pnl / 100.0 * position_size;
                  let profit_fee = quote_pnl.abs() * (self.fee / 100.0);
                  quote_pnl -= profit_fee;

                  *cum_capital += quote_pnl;
                  let quote = quote.get_mut(ticker).unwrap();
                  *quote += quote_pnl;

                  cum_quote.get_mut(ticker).unwrap().push(Data {
                    x: entry.date.to_unix_ms(),
                    y: trunc!(*quote, 2)
                  });
                  cum_pct.get_mut(ticker).unwrap().push(Data {
                    x: entry.date.to_unix_ms(),
                    y: trunc!(*cum_capital / initial_capital * 100.0 - 100.0, 2)
                  });
                  pct_per_trade.get_mut(ticker).unwrap().push(Data {
                    x: entry.date.to_unix_ms(),
                    y: trunc!(pct_pnl, 2)
                  });

                  // stop loss exit
                  let quantity = position_size / price_at_stop_loss;
                  let exit = Trade {
                    ticker: ticker.clone(),
                    date: candle.date,
                    side: Order::ExitLong,
                    quantity,
                    price: price_at_stop_loss,
                  };
                  active_trades.insert(ticker.clone(), None);
                  self.add_trade(exit, ticker.clone());
                }
              }
              Order::EnterShort => {
                // can only be stopped out if entering a short is allowed,
                // spot markets do not allow short selling
                if self.short_selling {
                  let pct_diff = (candle.high - entry.price) / entry.price * 100.0;
                  if pct_diff > stop_loss_pct {
                    let price_at_stop_loss = entry.price * (1.0 + stop_loss_pct / 100.0);
                    // longs are stopped out by the low
                    let pct_pnl = (price_at_stop_loss - entry.price) / entry.price * -1.0 * 100.0;
                    let position_size = match self.bet {
                      Bet::Static => static_capital,
                      Bet::Percent(pct) => *cum_capital.get(ticker).unwrap() * pct / 100.0
                    };

                    // add entry trade with updated quantity
                    let quantity = position_size / entry.price;
                    let updated_entry = Trade {
                      ticker: ticker.clone(),
                      date: entry.date,
                      side: entry.side,
                      quantity,
                      price: entry.price
                    };
                    self.add_trade(updated_entry, ticker.clone());

                    // fee on trade entry capital
                    let entry_fee = position_size.abs() * (self.fee / 100.0);
                    let cum_capital = cum_capital.get_mut(ticker).unwrap();
                    *cum_capital -= entry_fee;
                    // fee on profit made
                    let mut quote_pnl = pct_pnl / 100.0 * position_size;
                    let profit_fee = quote_pnl.abs() * (self.fee / 100.0);
                    quote_pnl -= profit_fee;

                    *cum_capital += quote_pnl;
                    let quote = quote.get_mut(ticker).unwrap();
                    *quote += quote_pnl;

                    cum_quote.get_mut(ticker).unwrap().push(Data {
                      x: entry.date.to_unix_ms(),
                      y: trunc!(*quote, 2)
                    });
                    cum_pct.get_mut(ticker).unwrap().push(Data {
                      x: entry.date.to_unix_ms(),
                      y: trunc!(*cum_capital / initial_capital * 100.0 - 100.0, 2)
                    });
                    pct_per_trade.get_mut(ticker).unwrap().push(Data {
                      x: entry.date.to_unix_ms(),
                      y: trunc!(pct_pnl, 2)
                    });

                    // stop loss exit
                    let quantity = position_size / price_at_stop_loss;
                    let exit = Trade {
                      ticker: ticker.clone(),
                      date: candle.date,
                      side: Order::ExitShort,
                      quantity,
                      price: price_at_stop_loss,
                    };
                    active_trades.insert(ticker.clone(), None);
                    self.add_trade(exit, ticker.clone());
                  }
                }
              }
              _ => ()
            }
          }

          // place new trade if signal is present
          let signals = self.strategy.process_candle(candle, Some(ticker.clone()))?;
          for signal in signals {
            match signal {
              Signal::EnterLong(info) => {
                // only place if no active trade to prevent pyramiding
                // todo: allow pyramiding to enable hedging
                if active_trades.get(&info.ticker).unwrap().is_none() {
                  let trade = Trade {
                    ticker: info.ticker.clone(),
                    date: info.date,
                    side: Order::EnterLong,
                    quantity: 0.0, // quantity doesn't matter, since exit trade recomputes it
                    price: info.price,
                  };
                  active_trades.insert(info.ticker.clone(), Some(trade.clone()));
                }
              },
              Signal::ExitLong(info) => {
                if let Some(entry) = active_trades.get(&info.ticker).unwrap() {
                  if entry.side == Order::EnterLong {
                    let pct_pnl = (info.price - entry.price) / entry.price * 100.0;
                    let position_size = match self.bet {
                      Bet::Static => static_capital,
                      Bet::Percent(pct) => *cum_capital.get(ticker).unwrap() * pct / 100.0
                    };

                    let quantity = position_size / entry.price;
                    let updated_entry = Trade {
                      ticker: ticker.clone(),
                      date: entry.date,
                      side: entry.side,
                      quantity,
                      price: entry.price
                    };
                    self.add_trade(updated_entry, info.ticker.clone());

                    // fee on trade entry capital
                    let entry_fee = position_size.abs() * (self.fee / 100.0);
                    let cum_capital = cum_capital.get_mut(ticker).unwrap();
                    *cum_capital -= entry_fee;
                    // fee on profit made
                    let mut quote_pnl = pct_pnl / 100.0 * position_size;
                    let profit_fee = quote_pnl.abs() * (self.fee / 100.0);
                    quote_pnl -= profit_fee;

                    *cum_capital += quote_pnl;
                    let quote = quote.get_mut(ticker).unwrap();
                    *quote += quote_pnl;

                    cum_quote.get_mut(ticker).unwrap().push(Data {
                      x: entry.date.to_unix_ms(),
                      y: trunc!(*quote, 2)
                    });
                    cum_pct.get_mut(ticker).unwrap().push(Data {
                      x: entry.date.to_unix_ms(),
                      y: trunc!(*cum_capital / initial_capital * 100.0 - 100.0, 2)
                    });
                    pct_per_trade.get_mut(ticker).unwrap().push(Data {
                      x: entry.date.to_unix_ms(),
                      y: trunc!(pct_pnl, 2)
                    });

                    let quantity = position_size / info.price;
                    let exit = Trade {
                      ticker: info.ticker.clone(),
                      date: info.date,
                      side: Order::ExitLong,
                      quantity,
                      price: info.price,
                    };
                    active_trades.insert(info.ticker.clone(), None);
                    self.add_trade(exit, info.ticker.clone());
                  }
                }
              },
              Signal::EnterShort(info) => {
                // only place if no active trade to prevent pyramiding
                // todo: allow pyramiding to enable hedging
                if active_trades.get(&info.ticker).unwrap().is_none() && self.short_selling {
                  let trade = Trade {
                    ticker: info.ticker.clone(),
                    date: info.date,
                    side: Order::EnterShort,
                    quantity: 0.0, // quantity doesn't matter, since exit trade recomputes it
                    price: info.price,
                  };
                  active_trades.insert(info.ticker.clone(), Some(trade.clone()));
                }
              },
              Signal::ExitShort(info) => {
                if let Some(entry) = active_trades.get(&info.ticker).unwrap() {
                  if entry.side == Order::EnterShort && self.short_selling { 
                    let pct_pnl = (info.price - entry.price) / entry.price * -1.0 * 100.0;
                    let position_size = match self.bet {
                      Bet::Static => static_capital,
                      Bet::Percent(pct) => *cum_capital.get(ticker).unwrap() * pct / 100.0
                    };

                    let quantity = position_size / entry.price;
                    let updated_entry = Trade {
                      ticker: ticker.clone(),
                      date: entry.date,
                      side: entry.side,
                      quantity,
                      price: entry.price
                    };
                    self.add_trade(updated_entry, info.ticker.clone());

                    // fee on trade entry capital
                    let entry_fee = position_size.abs() * (self.fee / 100.0);
                    let cum_capital = cum_capital.get_mut(ticker).unwrap();
                    *cum_capital -= entry_fee;
                    // fee on profit made
                    let mut quote_pnl = pct_pnl / 100.0 * position_size;
                    let profit_fee = quote_pnl.abs() * (self.fee / 100.0);
                    quote_pnl -= profit_fee;

                    *cum_capital += quote_pnl;
                    let quote = quote.get_mut(ticker).unwrap();
                    *quote += quote_pnl;

                    cum_quote.get_mut(ticker).unwrap().push(Data {
                      x: entry.date.to_unix_ms(),
                      y: trunc!(*quote, 2)
                    });
                    cum_pct.get_mut(ticker).unwrap().push(Data {
                      x: entry.date.to_unix_ms(),
                      y: trunc!(*cum_capital / initial_capital * 100.0 - 100.0, 2)
                    });
                    pct_per_trade.get_mut(ticker).unwrap().push(Data {
                      x: entry.date.to_unix_ms(),
                      y: trunc!(pct_pnl, 2)
                    });

                    let quantity = position_size / info.price;
                    let exit = Trade {
                      ticker: info.ticker.clone(),
                      date: info.date,
                      side: Order::ExitShort,
                      quantity,
                      price: info.price,
                    };
                    active_trades.insert(info.ticker.clone(), None);
                    self.add_trade(exit, info.ticker.clone());
                  }
                }
              },
              _ => ()
            }
          }
        }
      }
    }

    let cum_quote = cum_quote.iter().map(|(ticker, data)| {
      (ticker.clone(), Dataset::new(data.clone()))
    }).collect();
    let cum_pct = cum_pct.iter().map(|(ticker, data)| {
      (ticker.clone(), Dataset::new(data.clone()))
    }).collect();
    let pct_per_trade = pct_per_trade.iter().map(|(ticker, data)| {
      (ticker.clone(), Dataset::new(data.clone()))
    }).collect();
    Ok(Summary {
      cum_quote,
      cum_pct,
      pct_per_trade,
      trades: self.trades.clone()
    })
  }
}