#![allow(dead_code)]
#![allow(clippy::unnecessary_cast)]

use std::cell::Cell;
use time_series::{Candle, Data, Dataset, Op, Signal, SignalInfo, Summary, Time, trunc};
use std::fs::File;
use std::path::PathBuf;
use std::str::FromStr;
use lib::{Account};
use crate::Strategy;

pub struct CsvSeries {
  pub candles: Vec<Candle>,
  pub signals: Vec<Signal>,
  pub kagis: Vec<Data>,
  pub wmas: Vec<Data>
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
  Long,
  Short,
}

#[derive(Debug, Clone, Copy)]
pub struct Trade {
  pub date: Time,
  pub side: Order,
  /// base asset quantity
  pub quantity: f64,
  pub price: f64
}

#[derive(Debug, Clone, Default)]
pub struct Backtest<S: Strategy> {
  pub strategy: S,
  pub capital: f64,
  /// Fee in percentage
  pub fee: f64,
  pub compound: bool,
  pub leverage: u8,
  pub candles: Vec<Candle>,
  pub trades: Vec<Trade>,
  pub signals: Vec<Signal>
}
impl<S: Strategy> Backtest<S> {
  pub fn new(strategy: S, capital: f64, fee: f64, compound: bool, leverage: u8) -> Self {
    Self {
      strategy,
      capital,
      fee,
      compound,
      leverage,
      candles: vec![],
      trades: vec![],
      signals: vec![]
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
    ticker: String
  ) -> anyhow::Result<CsvSeries> {
    let file_buffer = File::open(csv_path)?;
    let mut csv = csv::Reader::from_reader(file_buffer);

    let mut headers = vec![];
    if let Ok(result) = csv.headers() {
      for header in result {
        headers.push(String::from(header));
      }
    }

    let mut signals = vec![];
    let mut candles = vec![];
    let mut kagis = vec![];
    let mut wmas = vec![];

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

      // if long and short signals from tradingview dreamrunner script are present,
      // it assumes they immediately follow the candle as the 5th and 6th indices
      if let (Ok(long), Ok(short)) = (u8::from_str(&record[5]), u8::from_str(&record[6])) {
        let long: bool = long == 1;
        let short: bool = short == 1;
        let signal = match (long, short) {
          (true, false) => Signal::Long(SignalInfo {
            price: candle.close, 
            date: candle.date,
            ticker: ticker.clone()
          }),
          (false, true) => Signal::Short(SignalInfo {
            price: candle.close, 
            date: candle.date,
            ticker: ticker.clone()
          }),
          _ => Signal::None
        };
        signals.push(signal);
      }

      // if Kagi and WMA plots from tradingview dreamrunner script are present,
      // it assumes they immediately follow the long/short signals as the 7th and 8th indices
      if let (Ok(kagi), Ok(wma)) = (f64::from_str(&record[7]), f64::from_str(&record[8])) {
        kagis.push(Data {
          x: date.to_unix_ms(),
          y: trunc!(kagi, 3)
        });
        wmas.push(Data {
          x: date.to_unix_ms(),
          y: trunc!(wma, 3)
        })
      }
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
    signals.retain(|signal| {
      match signal {
        Signal::Long(info) => {
          match (start_time, end_time) {
            (Some(start), Some(end)) => {
              info.date.to_unix_ms() > start.to_unix_ms() && info.date.to_unix_ms() < end.to_unix_ms()
            },
            (Some(start), None) => {
              info.date.to_unix_ms() > start.to_unix_ms()
            },
            (None, Some(end)) => {
              info.date.to_unix_ms() < end.to_unix_ms()
            },
            (None, None) => true
          }
        }
        Signal::Short(info) => {
          match (start_time, end_time) {
            (Some(start), Some(end)) => {
              info.date.to_unix_ms() > start.to_unix_ms() && info.date.to_unix_ms() < end.to_unix_ms()
            },
            (Some(start), None) => {
              info.date.to_unix_ms() > start.to_unix_ms()
            },
            (None, Some(end)) => {
              info.date.to_unix_ms() < end.to_unix_ms()
            },
            (None, None) => true
          }
        }
        Signal::None => false
      }
    });
    kagis.retain(|kagi| {
      match (start_time, end_time) {
        (Some(start), Some(end)) => {
          kagi.x > start.to_unix_ms() && kagi.x < end.to_unix_ms()
        },
        (Some(start), None) => {
          kagi.x > start.to_unix_ms()
        },
        (None, Some(end)) => {
          kagi.x < end.to_unix_ms()
        },
        (None, None) => true
      }
    });
    wmas.retain(|wma| {
      match (start_time, end_time) {
        (Some(start), Some(end)) => {
          wma.x > start.to_unix_ms() && wma.x < end.to_unix_ms()
        },
        (Some(start), None) => {
          wma.x > start.to_unix_ms()
        },
        (None, Some(end)) => {
          wma.x < end.to_unix_ms()
        },
        (None, None) => true
      }
    });

    Ok(CsvSeries {
      candles,
      signals,
      kagis,
      wmas
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

  pub fn add_candle(&mut self, candle: Candle) {
    self.candles.push(candle);
  }

  pub fn add_trade(&mut self, trade: Trade) {
    self.trades.push(trade);
  }

  pub fn add_signal(&mut self, signal: Signal) {
    self.signals.push(signal);
  }

  pub fn reset(&mut self) {
    self.trades.clear();
    self.signals.clear();
  }

  pub fn avg_quote_trade_size(&self) -> anyhow::Result<f64> {
    let avg = self.trades.iter().map(|t| {
      t.price * t.quantity
    }).sum::<f64>() / self.trades.len() as f64;
    Ok(trunc!(avg, 4))
  }

  pub fn buy_and_hold(
    &mut self,
    op: &Op
  ) -> anyhow::Result<Vec<Data>> {
    let candles = self.candles.clone();
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

    Ok(Dataset::new(data).translate(op))
  }

  pub fn backtest(
    &mut self,
    stop_loss: f64,
  ) -> anyhow::Result<()> {
    let mut active_trade: Option<Trade> = None;
    let capital = self.capital;

    let candles = self.candles.clone();
    for candle in candles {

      // check stop loss
      if let Some(trade) = &active_trade {
        let time = candle.date;
        match trade.side {
          Order::Long => {
            let price = candle.low;
            let pct_diff = (price - trade.price) / trade.price * 100.0;
            if pct_diff < stop_loss * -1.0 {
              let price_at_stop_loss = trade.price * (1.0 - stop_loss / 100.0);
              let trade = Trade {
                date: time,
                side: Order::Short,
                quantity: trade.quantity,
                price: price_at_stop_loss,
              };
              active_trade = None;
              self.add_trade(trade);
            }
          }
          Order::Short => {
            let price = candle.high;
            let pct_diff = (price - trade.price) / trade.price * 100.0;
            if pct_diff > stop_loss {
              let price_at_stop_loss = trade.price * (1.0 + stop_loss / 100.0);
              let trade = Trade {
                date: time,
                side: Order::Long,
                quantity: trade.quantity,
                price: price_at_stop_loss,
              };
              active_trade = None;
              self.add_trade(trade);
            }
          }
        }
      }

      // place new trade if signal is present
      let signal = self.strategy.process_candle(candle, self.ticker.clone())?[0].clone();
      match signal {
        Signal::Long(info) => {
          if let Some(trade) = &active_trade {
            if trade.side == Order::Long {
              continue;
            }
          }
          let quantity = capital / info.price;
          let trade = Trade {
            date: info.date,
            side: Order::Long,
            quantity,
            price: info.price,
          };
          active_trade = Some(trade);
          self.add_trade(trade);
        },
        Signal::Short(info) => {
          if let Some(trade) = &active_trade {
            if trade.side == Order::Short {
              continue;
            }
          }
          let quantity = capital / info.price;
          let trade = Trade {
            date: info.date,
            side: Order::Short,
            quantity,
            price: info.price,
          };
          active_trade = Some(trade);
          self.add_trade(trade);
        },
        Signal::None => ()
      }
    }

    Ok(())
  }

  /// Assumes trading with static position size (e.g. $1000 every trade) and not reinvesting profits.
  pub fn backtest_tradingview(&self, compound: bool) -> anyhow::Result<Summary> {
    let mut capital = self.capital;
    let initial_capital = capital;
    let signals = &self.signals;

    let mut quote = 0.0;
    let mut cum_pct = vec![];
    let mut cum_quote = vec![];
    let mut pct_per_trade = vec![];

    // filter out None signals
    let signals = signals.iter().filter(|s| {
      !matches!(s, Signal::None)
    }).collect::<Vec<&Signal>>();

    for signals in signals.windows(2) {
      let entry = &signals[0];
      let exit = &signals[1];
      match (entry, exit) {
        (Signal::Long((entry, entry_date)), Signal::Short((exit, _))) => {
          let factor = 1.0;

          let pct_pnl = ((exit - entry) / entry * factor) * 100.0;
          let mut position_size = match compound {
            true => capital,
            false => initial_capital
          };
          position_size -= position_size.abs() * (self.fee / 100.0); // take exchange fee on trade entry capital
          let quote_pnl = pct_pnl / 100.0 * position_size;
          // quote_pnl -= quote_pnl.abs() * (self.fee / 100.0); // take exchange fee on trade exit capital

          capital += quote_pnl;
          quote += quote_pnl;

          cum_quote.push(Data {
            x: entry_date.to_unix_ms(),
            y: trunc!(quote, 4)
          });
          cum_pct.push(Data {
            x: entry_date.to_unix_ms(),
            y: trunc!(capital / initial_capital * 100.0 - 100.0, 4)
          });
          pct_per_trade.push(Data {
            x: entry_date.to_unix_ms(),
            y: trunc!(pct_pnl, 4)
          });
        },
        (Signal::Short((entry, entry_date)), Signal::Long((exit, _))) => {
          let factor = -1.0;

          let pct_pnl = ((exit - entry) / entry * factor) * 100.0;
          let mut position_size = match compound {
            true => capital,
            false => initial_capital
          };
          position_size -= position_size.abs() * (self.fee / 100.0); // take exchange fee on trade entry capital
          let quote_pnl = pct_pnl / 100.0 * position_size;

          capital += quote_pnl;
          quote += quote_pnl;

          cum_quote.push(Data {
            x: entry_date.to_unix_ms(),
            y: trunc!(quote, 4)
          });
          cum_pct.push(Data {
            x: entry_date.to_unix_ms(),
            y: trunc!(capital / initial_capital * 100.0 - 100.0, 4)
          });
          pct_per_trade.push(Data {
            x: entry_date.to_unix_ms(),
            y: trunc!(pct_pnl, 4)
          });
        },
        _ => continue
      }
    }

    Ok(Summary {
      avg_trade_size: self.avg_quote_trade_size()?,
      cum_quote: Dataset::new(cum_quote),
      cum_pct: Dataset::new(cum_pct),
      pct_per_trade: Dataset::new(pct_per_trade)
    })
  }

  /// If compounded, assumes trading profits are 100% reinvested.
  /// If not compounded, assumed trading with initial capital (e.g. $1000 every trade) and not reinvesting profits.
  pub fn summary(&mut self) -> anyhow::Result<Summary> {
    let mut capital = self.capital * self.leverage as f64;
    let initial_capital = capital;

    let mut quote = 0.0;
    let mut cum_pct = vec![];
    let mut cum_quote = vec![];
    let mut pct_per_trade = vec![];

    let slice = &mut self.trades.clone()[..];
    let slice_of_cells: &[Cell<Trade>] = Cell::from_mut(slice).as_slice_of_cells();
    for trades in slice_of_cells.windows(2) {
      let exit = &trades[1];
      let entry = &trades[0];
      let factor = match entry.get().side {
        Order::Long => 1.0,
        Order::Short => -1.0,
      };
      let pct_pnl = ((exit.get().price - entry.get().price) / entry.get().price * factor) * 100.0;
      let position_size = match self.compound {
        true => capital,
        false => initial_capital
      };
      
      let quantity = position_size / entry.get().price;
      let updated_entry = Trade {
        date: entry.get().date,
        side: entry.get().side,
        quantity,
        price: entry.get().price
      };
      Cell::swap(entry, &Cell::from(updated_entry));
      
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
        x: entry.get().date.to_unix_ms(),
        y: trunc!(quote, 4)
      });
      cum_pct.push(Data {
        x: entry.get().date.to_unix_ms(),
        y: trunc!(capital / initial_capital * 100.0 - 100.0, 4)
      });
      pct_per_trade.push(Data {
        x: entry.get().date.to_unix_ms(),
        y: trunc!(pct_pnl, 4)
      });

      let quantity = capital / exit.get().price;
      let updated_exit = Trade {
        date: exit.get().date,
        side: exit.get().side,
        quantity,
        price: exit.get().price
      };
      Cell::swap(exit, &Cell::from(updated_exit));
    }

    // set self.trades to slice_of_cells
    self.trades = slice_of_cells.iter().map(|cell| cell.get()).collect();

    Ok(Summary {
      avg_trade_size: self.avg_quote_trade_size()?,
      cum_quote: Dataset::new(cum_quote),
      cum_pct: Dataset::new(cum_pct),
      pct_per_trade: Dataset::new(pct_per_trade)
    })
  }
}