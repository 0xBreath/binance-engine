#![allow(dead_code)]
#![allow(clippy::unnecessary_cast)]

use std::collections::HashMap;
use time_series::{Candle, Data, Dataset, Order, Signal, Summary, Time, Trade, trunc};
use std::fs::File;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::str::FromStr;
use lib::{Account};
use crate::Strategy;

pub struct CsvSeries {
  pub candles: Vec<Candle>,
}

#[derive(Debug, Clone, Default)]
pub struct Backtest<T, S: Strategy<T>> {
  pub strategy: S,
  pub capital: f64,
  /// Fee in percentage
  pub fee: f64,
  /// If compounded, assumes trading profits are 100% reinvested.
  /// If not compounded, assumed trading with initial capital (e.g. $1000 every trade) and not reinvesting profits.
  pub compound: bool,
  pub leverage: u8,
  /// False if spot trading, true if margin trading which allows short selling
  pub short_selling: bool,
  pub candles: HashMap<String, Vec<Candle>>,
  pub trades: HashMap<String, Vec<Trade>>,
  pub signals: HashMap<String, Vec<Signal>>,
  _data: PhantomData<T>
}
impl<T, S: Strategy<T>> Backtest<T, S> {
  pub fn new(strategy: S, capital: f64, fee: f64, compound: bool, leverage: u8, short_selling: bool) -> Self {
    Self {
      strategy,
      capital,
      fee,
      compound,
      leverage,
      short_selling,
      candles: HashMap::new(),
      trades: HashMap::new(),
      signals: HashMap::new(),
      _data: PhantomData
    }
  }

  /// Read candles from CSV file.
  /// Handles duplicate candles and sorts candles by date.
  /// Expects date of candle to be in UNIX timestamp format.
  /// CSV format: date,open,high,low,close,volume
  pub fn csv_series(
    csv_path: &PathBuf,
    start_time: Option<Time>,
    end_time: Option<Time>,
    _ticker: String
  ) -> anyhow::Result<CsvSeries> {
    let file_buffer = File::open(csv_path)?;
    let mut csv = csv::Reader::from_reader(file_buffer);

    let mut headers = vec![];
    if let Ok(result) = csv.headers() {
      for header in result {
        headers.push(String::from(header));
      }
    }

    let mut candles = vec![];

    for record in csv.records().flatten() {
      let date = Time::from_unix(
        record[0]
          .parse::<i64>()
          .expect("failed to parse candle UNIX timestamp into i64"),
      );
      let volume = None;
      let candle = Candle {
        date,
        open: f64::from_str(&record[1])?,
        high: f64::from_str(&record[2])?,
        low: f64::from_str(&record[3])?,
        close: f64::from_str(&record[4])?,
        volume,
      };
      candles.push(candle);
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

    Ok(CsvSeries {
      candles,
    })
  }

  pub async fn add_klines(
    &mut self,
    account: &Account,
    start_time: Option<Time>,
    end_time: Option<Time>
  ) -> anyhow::Result<Vec<Candle>> {
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
  ) -> anyhow::Result<HashMap<String, Vec<Data<f64>>>> {
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
    stop_loss: f64,
  ) -> anyhow::Result<Summary> {
    let candles = self.candles.clone();
    
    let static_capital = self.capital * self.leverage as f64;
    let initial_capital = self.capital;

    let mut cum_capital: HashMap<String, f64> = HashMap::new();
    let mut quote: HashMap<String, f64> = HashMap::new();
    let mut cum_pct: HashMap<String, Vec<Data<f64>>> = HashMap::new();
    let mut cum_quote: HashMap<String, Vec<Data<f64>>> = HashMap::new();
    let mut pct_per_trade:  HashMap<String, Vec<Data<f64>>> = HashMap::new();

    let pre_backtest = std::time::SystemTime::now();
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
      let mut index_iter_times = vec![];
      for i in 0..length {
        // Access the i-th element of each vector to simulate getting price update
        // for every ticker at roughly the same time
        let mut iter_times: Vec<u128> = vec![];
        for (ticker, candles) in candles.iter() {
          let now = std::time::SystemTime::now();
          let candle = candles[i];

          // check if stop loss is hit
          if let Some(entry) = active_trades.get(ticker).unwrap() {
            match entry.side {
              Order::EnterLong => {
                let pct_diff = (candle.low - entry.price) / entry.price * 100.0;
                if pct_diff < stop_loss * -1.0 {
                  let price_at_stop_loss = entry.price * (1.0 - stop_loss / 100.0);
                  // longs are stopped out by the low
                  let pct_pnl = (price_at_stop_loss - entry.price) / entry.price * 100.0;
                  let position_size = match self.compound {
                    true => *cum_capital.get(ticker).unwrap(),
                    false => static_capital
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
                  if pct_diff > stop_loss {
                    let price_at_stop_loss = entry.price * (1.0 + stop_loss / 100.0);
                    // longs are stopped out by the low
                    let pct_pnl = (price_at_stop_loss - entry.price) / entry.price * -1.0 * 100.0;
                    let position_size = match self.compound {
                      true => *cum_capital.get(ticker).unwrap(),
                      false => static_capital
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
                    let position_size = match self.compound {
                      true => *cum_capital.get(ticker).unwrap(),
                      false => static_capital
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
                    let position_size = match self.compound {
                      true => *cum_capital.get(ticker).unwrap(),
                      false => static_capital
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
          iter_times.push(now.elapsed().unwrap().as_micros());
        }
        let avg = iter_times.iter().sum::<u128>() as f64 / iter_times.len() as f64;
        index_iter_times.push(avg);
      }
      if !index_iter_times.is_empty() {
        println!(
          "Average index iteration time: {}us for {} indices",
          trunc!(index_iter_times.iter().sum::<f64>() / index_iter_times.len() as f64, 1),
          index_iter_times.len()
        );
      }
    }
    println!("Backtest lasted: {:?}s", trunc!(pre_backtest.elapsed().unwrap().as_secs_f64(), 1));

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