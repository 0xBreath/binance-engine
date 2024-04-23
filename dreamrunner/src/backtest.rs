#![allow(dead_code)]
#![allow(clippy::unnecessary_cast)]

use time_series::{Candle, Data, Kagi, KagiDirection, Pnl, RollingCandles, Signal, Source, Time, trunc};
use std::fs::File;
use std::path::PathBuf;
use std::str::FromStr;
use lib::{Account};
use crate::dreamrunner::Dreamrunner;


#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Order {
  Long,
  Short,
}

#[derive(Debug, Clone)]
pub struct Trade {
  pub date: Time,
  pub side: Order,
  /// base asset quantity
  pub quantity: f64,
  pub price: f64
}

#[derive(Debug, Clone, Default)]
pub struct Backtest {
  pub capital: f64,
  pub candles: Vec<Candle>,
  pub trades: Vec<Trade>,
  pub signals: Vec<Signal>
}
impl Backtest {
  pub fn new(capital: f64) -> Self {
    Self {
      capital,
      candles: Vec::new(),
      trades: Vec::new(),
      signals: Vec::new()
    }
  }

  /// Read candles from CSV file.
  /// Handles duplicate candles and sorts candles by date.
  /// Expects date of candle to be in UNIX timestamp format.
  /// CSV format: date,open,high,low,close,volume
  pub fn add_csv_series(&mut self, csv_path: &PathBuf, start_time: Option<Time>, end_time: Option<Time>) -> anyhow::Result<()> {
    let file_buffer = File::open(csv_path)?;
    let mut csv = csv::Reader::from_reader(file_buffer);

    let mut headers = Vec::new();
    if let Ok(result) = csv.headers() {
      for header in result {
        headers.push(String::from(header));
      }
    }

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
      // if long and short signals from tradingview backtest are present,
      // it assumes they immediately follow the candle as the 5th and 6th indices
      if let (Ok(long), Ok(short)) = (u8::from_str(&record[5]), u8::from_str(&record[6])) {
        let long: bool = long == 1;
        let short: bool = short == 1;
        let signal = match (long, short) {
          (true, false) => Signal::Long((candle.close, candle.date)),
          (false, true) => Signal::Short((candle.close, candle.date)),
          _ => Signal::None
        };
        self.add_signal(signal);
      }
      
      self.add_candle(candle);
    }
    // only take candles greater than a timestamp
    self.candles.retain(|candle| {
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

    Ok(())
  }
  
  pub async fn add_klines(&mut self, account: &Account, start_time: Option<Time>, end_time: Option<Time>) -> anyhow::Result<()> {
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
    
    for kline in klines.into_iter() {
      self.add_candle(kline.to_candle());
    }
    // only take candles greater than a timestamp
    self.candles.retain(|candle| {
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
    
    Ok(())
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

  pub fn avg_quote_trade_size(&self) -> anyhow::Result<f64> {
    let avg = self.trades.iter().map(|t| {
      t.price * t.quantity
    }).sum::<f64>() / self.trades.len() as f64;
    Ok(trunc!(avg, 4))
  }
  
  /// Assumes trading with static position size (e.g. $1000 every trade) and not reinvesting profits.
  pub fn simulate_tradingview(&self) -> anyhow::Result<Pnl> {
    let capital = self.capital;
    let signals = &self.signals;

    let mut quote = 0.0;
    let mut pct = 0.0;
    let mut pct_data = Vec::new();
    let mut quote_data = Vec::new();
    let mut winners = 0;
    let mut total_trades = 0;
    
    // filter out None signals
    let signals = signals.iter().filter(|s| {
      !matches!(s, Signal::None)
    }).collect::<Vec<&Signal>>();

    for signals in signals.windows(2) {
      let exit = &signals[1];
      let entry = &signals[0];
      match (entry, exit) {
        (Signal::Long((entry, date)), Signal::Short((exit, _))) => {
          let factor = 1.0;

          let pct_pnl = ((exit - entry) / entry * factor) * 100.0;
          // let quote_pnl = pct_pnl / 100.0 * entry.quantity * entry.price;
          let quote_pnl = pct_pnl / 100.0 * capital;

          quote += quote_pnl;
          pct = quote / capital * 100.0;

          if quote_pnl > 0.0 {
            winners += 1;
          }
          total_trades += 1;

          pct_data.push(Data {
            x: date.to_unix_ms(),
            y: trunc!(pct, 4)
          });
          quote_data.push(Data {
            x: date.to_unix_ms(),
            y: trunc!(quote, 4)
          });
        },
        (Signal::Short((entry, date)), Signal::Long((exit, _))) => {
          let factor = -1.0;

          let pct_pnl = ((exit - entry) / entry * factor) * 100.0;
          // let quote_pnl = pct_pnl / 100.0 * entry.quantity * entry.price;
          let quote_pnl = pct_pnl / 100.0 * capital;

          quote += quote_pnl;
          pct = quote / capital * 100.0;

          if quote_pnl > 0.0 {
            winners += 1;
          }
          total_trades += 1;

          pct_data.push(Data {
            x: date.to_unix_ms(),
            y: trunc!(pct, 4)
          });
          quote_data.push(Data {
            x: date.to_unix_ms(),
            y: trunc!(quote, 4)
          });
        },
        _ => continue
      }
    }
    let avg_pct_pnl = pct_data.iter().map(|d| d.y).sum::<f64>() / pct_data.len() as f64;
    let win_rate = (winners as f64 / total_trades as f64) * 100.0;
    let max_pct_drawdown = pct_data.iter().fold((0.0, 0.0), |(max, drawdown), d| {
      let max = (max as f64).max(d.y);
      let drawdown = (drawdown as f64).min(d.y - max);
      (max, drawdown)
    }).1;
    Ok(Pnl {
      quote: trunc!(quote, 4),
      pct: trunc!(pct, 4),
      pct_data,
      win_rate: trunc!(win_rate, 4),
      total_trades,
      avg_quote_trade_size: self.avg_quote_trade_size()?,
      avg_pct_pnl: trunc!(avg_pct_pnl, 4),
      max_pct_drawdown: trunc!(max_pct_drawdown, 4),
      quote_data,
    })
  }

  /// Assumes trading with static position size (e.g. $1000 every trade) and not reinvesting profits.
  pub fn pnl(&self) -> anyhow::Result<Pnl> {
    let trades = &self.trades;
    let capital = self.capital;
    
    let mut quote = 0.0;
    let mut pct = 0.0;
    let mut pct_data = Vec::new();
    let mut quote_data = Vec::new();
    let mut winners = 0;
    let mut total_trades = 0;
    
    for trades in trades.windows(2) {
      let exit = &trades[1];
      let entry = &trades[0];
      let factor = match entry.side {
        Order::Long => 1.0,
        Order::Short => -1.0,
      };
      let pct_pnl = ((exit.price - entry.price) / entry.price * factor) * 100.0;
      // let quote_pnl = pct_pnl / 100.0 * entry.quantity * entry.price;
      let quote_pnl = pct_pnl / 100.0 * capital;

      quote += quote_pnl;
      pct = quote / capital * 100.0;
      
      if quote_pnl > 0.0 {
        winners += 1;
      }
      total_trades += 1;

      pct_data.push(Data {
        x: entry.date.to_unix_ms(),
        y: trunc!(pct, 4)
      });
      quote_data.push(Data {
        x: entry.date.to_unix_ms(),
        y: trunc!(quote, 4)
      });
    }
    let avg_pct_pnl = pct_data.iter().map(|d| d.y).sum::<f64>() / pct_data.len() as f64;
    let win_rate = (winners as f64 / total_trades as f64) * 100.0;
    let max_pct_drawdown = pct_data.iter().fold((0.0, 0.0), |(max, drawdown), d| {
      let max = (max as f64).max(d.y);
      let drawdown = (drawdown as f64).min(d.y - max);
      (max, drawdown)
    }).1;
    Ok(Pnl {
      quote: trunc!(quote, 4),
      pct: trunc!(pct, 4),
      pct_data,
      win_rate: trunc!(win_rate, 4),
      total_trades,
      avg_quote_trade_size: self.avg_quote_trade_size()?,
      avg_pct_pnl: trunc!(avg_pct_pnl, 4),
      max_pct_drawdown: trunc!(max_pct_drawdown, 4),
      quote_data,
    })
  }

  pub fn wmas(
    &mut self,
    wma_period: usize,
    k_rev: f64,
    k_src: Source,
    ma_src: Source
  ) -> anyhow::Result<Vec<Data>> {
    let mut period = RollingCandles::new(wma_period + 1);
    let dreamrunner = Dreamrunner {
      k_rev,
      k_src,
      ma_src
    };
    let mut data = Vec::new();

    let candles = self.candles.clone();
    for candle in candles {
      period.push(candle);
      let period_from_curr: Vec<&Candle> = period.vec.range(0..period.vec.len() - 1).collect();
      data.push(Data {
        x: candle.date.to_unix_ms(),
        y: dreamrunner.wma(&period_from_curr)
      });
    }
    Ok(data)
  }

  pub fn kagis(
    &mut self,
    wma_period: usize,
    k_rev: f64,
    k_src: Source,
    ma_src: Source
  ) -> anyhow::Result<Vec<Data>> {
    let mut period = RollingCandles::new(wma_period + 1);
    let mut kagi = Kagi {
      line: self.candles.first().unwrap().low,
      direction: KagiDirection::Down,
    };
    let dreamrunner = Dreamrunner {
      k_rev,
      k_src,
      ma_src
    };
    let mut data = Vec::new();

    let candles = self.candles.clone();
    for candle in candles {
      period.push(candle);
      let _ = dreamrunner.signal(&mut kagi, &period)?;
      data.push(Data {
        x: candle.date.to_unix_ms(),
        y: kagi.line
      });
    }
    Ok(data)
  }

  pub fn simulate(
    &mut self,
    wma_period: usize,
    k_rev: f64,
    k_src: Source,
    ma_src: Source
  ) -> anyhow::Result<()> {
    let mut active_trade: Option<Trade> = None;
    let mut period = RollingCandles::new(wma_period + 1);
    let mut kagi = Kagi {
      line: self.candles.first().unwrap().low,
      direction: KagiDirection::Down,
    };
    let dreamrunner = Dreamrunner {
      k_rev,
      k_src,
      ma_src
    };
    let capital = self.capital;

    let candles = self.candles.clone();
    for candle in candles {
      period.push(candle);

      let signal = dreamrunner.signal(&mut kagi, &period)?;

      match signal {
        Signal::Long((price, time)) => {
          match &active_trade {
            Some(trade) => {
              if trade.side == Order::Long {
                continue;
              }
            }
            None => ()
          }
          let quantity = capital / price;
          let trade = Trade {
            date: time,
            side: Order::Long,
            quantity,
            price,
          };
          active_trade = Some(trade.clone());
          self.add_trade(trade);
        },
        Signal::Short((price, time)) => {
          if let Some(trade) = &active_trade {
            if trade.side == Order::Short {
              continue;
            }
            let quantity = capital / price;

            let trade = Trade {
              date: time,
              side: Order::Short,
              quantity,
              price,
            };
            active_trade = Some(trade.clone());
            self.add_trade(trade);
          }
        },
        Signal::None => ()
      }
    }

    Ok(())
  }
}




#[tokio::test]
async fn backtest_dreamrunner() -> anyhow::Result<()> {
  use super::*;
  use time_series::{Day, Month, Plot};
  dotenv::dotenv().ok();

  let capital = 1_000.0;
  let mut backtest = Backtest::new(capital);

  let start_time = Time::new(2023, &Month::from_num(9), &Day::from_num(1), None, None, None);
  // let start_time = Time::new(2024, &Month::from_num(4), &Day::from_num(15), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(22), None, None, None);

  let out_file = "solusdt_30m.csv";
  let csv = PathBuf::from(out_file);
  backtest.add_csv_series(&csv, Some(start_time), Some(end_time))?;

  let earliest = backtest.candles.first().unwrap().date;
  let latest = backtest.candles.last().unwrap().date;
  println!("kline history: {} - {}", earliest.to_string(), latest.to_string());
  

  let k_rev = 0.001;
  let k_src = Source::Close;
  let ma_src = Source::Open;
  let wma_period = 5;

  backtest.simulate(
    wma_period,
    k_rev,
    k_src,
    ma_src
  )?;
  let summary = backtest.pnl()?;
  println!("==== Dreamrunner Backtest ====");
  summary.print();
  
  Plot::plot(
    vec![summary.quote_data],
    "dreamrunner_backtest.png",
    "Dreamrunner Backtest",
    "Equity"
  )?;

  let wmas = backtest.wmas(wma_period, k_rev, k_src, ma_src)?;
  let kagis = backtest.kagis(wma_period, k_rev, k_src, ma_src)?;
  Plot::plot(
    vec![wmas, kagis],
    "strategy.png",
    "Strategy",
    "USDT Price"
  )?;
  
  let tradingview_backtest = backtest.simulate_tradingview()?;
  println!("==== Tradingview Backtest ====");
  tradingview_backtest.print();
  Plot::plot(
    vec![tradingview_backtest.quote_data],
    "tradingview_backtest.png",
    "Tradingview Backtest",
    "Equity"
  )?;

  Ok(())
}
