#![allow(dead_code)]
#![allow(clippy::unnecessary_cast)]

use std::collections::HashMap;
use time_series::{Candle, Data, Dataset, Signal, Summary, Time, trunc};
use std::fs::File;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::str::FromStr;
use lib::{Account};
use crate::Strategy;

pub struct CsvSeries {
  pub candles: Vec<Candle>,
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
  Long,
  Short,
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

#[derive(Debug, Clone, Default)]
pub struct Backtest<T, S: Strategy<T>> {
  pub strategy: S,
  pub capital: f64,
  /// Fee in percentage
  pub fee: f64,
  pub compound: bool,
  pub leverage: u8,
  pub candles: HashMap<String, Vec<Candle>>,
  pub trades: HashMap<String, Vec<Trade>>,
  pub signals: HashMap<String, Vec<Signal>>,
  _data: PhantomData<T>
}
impl<T, S: Strategy<T>> Backtest<T, S> {
  pub fn new(strategy: S, capital: f64, fee: f64, compound: bool, leverage: u8) -> Self {
    Self {
      strategy,
      capital,
      fee,
      compound,
      leverage,
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
    &mut self,
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

  pub fn avg_quote_trade_size(&self, ticker: String) -> anyhow::Result<f64> {
    let trades = self.trades.get(&ticker).ok_or(anyhow::anyhow!("No trades for ticker"))?;
    let avg = trades.iter().map(|t| {
      t.price * t.quantity
    }).sum::<f64>() / trades.len() as f64;
    Ok(trunc!(avg, 4))
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
  ) -> anyhow::Result<()> {
    let capital = self.capital;
    let candles = self.candles.clone();
    
    let pre_backtest = std::time::SystemTime::now();
    if let Some((_, first_series)) = candles.iter().next() {
      let length = first_series.len();

      let mut active_trades: HashMap<String, Option<Trade>> = HashMap::new();
      for (ticker, _) in candles.iter() {
        // populate active trades with None values for each ticker so getter doesn't panic
        active_trades.insert(ticker.clone(), None);
        // populate trades with empty vec for each ticker so getter doesn't panic
        self.trades.insert(ticker.clone(), vec![]);
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
          if let Some(trade) = active_trades.get(ticker).unwrap() {
            let time = candle.date;
            match trade.side {
              Order::Long => {
                let price = candle.low;
                let pct_diff = (price - trade.price) / trade.price * 100.0;
                if pct_diff < stop_loss * -1.0 {
                  let price_at_stop_loss = trade.price * (1.0 - stop_loss / 100.0);
                  let trade = Trade {
                    ticker: ticker.clone(),
                    date: time,
                    side: Order::Short,
                    quantity: trade.quantity,
                    price: price_at_stop_loss,
                  };
                  active_trades.insert(ticker.clone(), None);
                  self.add_trade(trade, ticker.clone());
                }
              }
              Order::Short => {
                let price = candle.high;
                let pct_diff = (price - trade.price) / trade.price * 100.0;
                if pct_diff > stop_loss {
                  let price_at_stop_loss = trade.price * (1.0 + stop_loss / 100.0);
                  let trade = Trade {
                    ticker: ticker.clone(),
                    date: time,
                    side: Order::Long,
                    quantity: trade.quantity,
                    price: price_at_stop_loss,
                  };
                  active_trades.insert(ticker.clone(), None);
                  self.add_trade(trade, ticker.clone());
                }
              }
            }
          }
        
          // place new trade if signal is present
          let signals = self.strategy.process_candle(candle, Some(ticker.clone()))?;
          for signal in signals {
            match signal {
              Signal::Long(info) => {
                if &info.ticker == ticker {
                  if let Some(trade) = active_trades.get(ticker).unwrap() {
                    if trade.side == Order::Long {
                      continue;
                    }
                  }
                  let quantity = capital / info.price;
                  let trade = Trade {
                    ticker: info.ticker.clone(),
                    date: info.date,
                    side: Order::Long,
                    quantity,
                    price: info.price,
                  };
                  active_trades.insert(ticker.clone(), Some(trade.clone()));
                  self.add_trade(trade, ticker.clone());
                }
              },
              Signal::Short(info) => {
                if &info.ticker == ticker {
                  if let Some(trade) = active_trades.get(ticker).unwrap() {
                    if trade.side == Order::Short {
                      continue;
                    }
                  }
                  let quantity = capital / info.price;
                  let trade = Trade {
                    ticker: ticker.clone(),
                    date: info.date,
                    side: Order::Short,
                    quantity,
                    price: info.price,
                  };
                  active_trades.insert(ticker.clone(), Some(trade.clone()));
                  self.add_trade(trade, ticker.clone());
                }
              },
              Signal::None => ()
            }
          }
          iter_times.push(now.elapsed().unwrap().as_micros());
        }
        index_iter_times.push(iter_times.iter().sum::<u128>() as f64 / iter_times.len() as f64);
      }
      if !index_iter_times.is_empty() {
        println!(
          "Average index iteration time: {}us for {} indices", 
          trunc!(index_iter_times.iter().sum::<f64>() / index_iter_times.len() as f64, 1),
          index_iter_times.len()
        );
      }
    }
    println!("Backtest lasted: {:?}s", pre_backtest.elapsed().unwrap().as_secs_f64());

    Ok(())
  }

  /// If compounded, assumes trading profits are 100% reinvested.
  /// If not compounded, assumed trading with initial capital (e.g. $1000 every trade) and not reinvesting profits.
  pub fn summary(&mut self, ticker: String) -> anyhow::Result<Summary> {
    let mut capital = self.capital * self.leverage as f64;
    let initial_capital = capital;

    let mut quote = 0.0;
    let mut cum_pct = vec![];
    let mut cum_quote = vec![];
    let mut pct_per_trade = vec![];

    let mut updated_trades = vec![];
    let trades = self.trades.get(&ticker).ok_or(anyhow::anyhow!("No trades for ticker"))?;
    for trades in trades.windows(2) {
      let exit = &trades[1];
      let entry = &trades[0];
      let factor = match entry.side {
        Order::Long => 1.0,
        Order::Short => -1.0,
      };
      let pct_pnl = ((exit.price - entry.price) / entry.price * factor) * 100.0;
      let position_size = match self.compound {
        true => capital,
        false => initial_capital
      };
      
      let quantity = position_size / entry.price;
      let updated_entry = Trade {
        ticker: ticker.clone(),
        date: entry.date,
        side: entry.side,
        quantity,
        price: entry.price
      };
      updated_trades.push(updated_entry);
      
      // fee on trade entry capital
      let entry_fee = position_size.abs() * (self.fee / 100.0);
      capital -= entry_fee;
      
      // fee on profit made
      let mut quote_pnl = pct_pnl / 100.0 * position_size;
      let profit_fee = quote_pnl.abs() * (self.fee / 100.0);
      quote_pnl -= profit_fee;

      capital += quote_pnl;
      quote += quote_pnl;

      cum_quote.push(Data {
        x: entry.date.to_unix_ms(),
        y: trunc!(quote, 4)
      });
      cum_pct.push(Data {
        x: entry.date.to_unix_ms(),
        y: trunc!(capital / initial_capital * 100.0 - 100.0, 4)
      });
      pct_per_trade.push(Data {
        x: entry.date.to_unix_ms(),
        y: trunc!(pct_pnl, 4)
      });

      let quantity = capital / exit.price;
      let updated_exit = Trade {
        ticker: ticker.clone(),
        date: exit.date,
        side: exit.side,
        quantity,
        price: exit.price
      };
      updated_trades.push(updated_exit);
    }

    self.trades.insert(ticker.clone(), updated_trades);

    Ok(Summary {
      avg_trade_size: self.avg_quote_trade_size(ticker.clone())?,
      cum_quote: Dataset::new(cum_quote),
      cum_pct: Dataset::new(cum_pct),
      pct_per_trade: Dataset::new(pct_per_trade)
    })
  }
}