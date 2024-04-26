#![allow(dead_code)]
#![allow(clippy::unnecessary_cast)]

use std::cell::Cell;
use time_series::{Candle, Data, Dataset, Kagi, KagiDirection, Op, RollingCandles, Signal, Source, Summary, Time, trunc};
use std::fs::File;
use std::path::PathBuf;
use std::str::FromStr;
use lib::{Account};
use crate::dreamrunner::Dreamrunner;

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
pub struct Backtest {
  pub capital: f64,
  /// Fee in percentage
  pub fee: f64,
  pub candles: Vec<Candle>,
  pub trades: Vec<Trade>,
  pub signals: Vec<Signal>
}
impl Backtest {
  pub fn new(capital: f64, fee: f64) -> Self {
    Self {
      capital,
      fee,
      candles: Vec::new(),
      trades: Vec::new(),
      signals: Vec::new()
    }
  }

  /// Read candles from CSV file.
  /// Handles duplicate candles and sorts candles by date.
  /// Expects date of candle to be in UNIX timestamp format.
  /// CSV format: date,open,high,low,close,volume
  pub fn add_csv_series(
    &mut self,
    csv_path: &PathBuf,
    start_time: Option<Time>,
    end_time: Option<Time>
  ) -> anyhow::Result<CsvSeries> {
    let file_buffer = File::open(csv_path)?;
    let mut csv = csv::Reader::from_reader(file_buffer);

    let mut headers = Vec::new();
    if let Ok(result) = csv.headers() {
      for header in result {
        headers.push(String::from(header));
      }
    }

    let mut signals = Vec::new();
    let mut candles = Vec::new();
    let mut kagis = Vec::new();
    let mut wmas = Vec::new();

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
          (true, false) => Signal::Long((candle.close, candle.date)),
          (false, true) => Signal::Short((candle.close, candle.date)),
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

    let mut candles = Vec::new();
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

  /// Assumes trading with static position size (e.g. $1000 every trade) and not reinvesting profits.
  pub fn backtest_tradingview(&self) -> anyhow::Result<Summary> {
    let capital = self.capital;
    let signals = &self.signals;

    let mut quote = 0.0;
    let mut pnl_data = Vec::new();
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
          let mut quote_pnl = pct_pnl / 100.0 * capital;
          quote_pnl -= quote_pnl.abs() * self.fee / 100.0;

          quote += quote_pnl;

          if quote_pnl > 0.0 {
            winners += 1;
          }
          total_trades += 1;

          quote_data.push(Data {
            x: date.to_unix_ms(),
            y: trunc!(quote, 4)
          });
        },
        (Signal::Short((entry, date)), Signal::Long((exit, _))) => {
          let factor = -1.0;

          let pct_pnl = ((exit - entry) / entry * factor) * 100.0;
          let mut quote_pnl = pct_pnl / 100.0 * capital;
          quote_pnl -= quote_pnl.abs() * self.fee / 100.0;

          quote += quote_pnl;

          if quote_pnl > 0.0 {
            winners += 1;
          }
          total_trades += 1;

          quote_data.push(Data {
            x: date.to_unix_ms(),
            y: trunc!(quote, 4)
          });
          pnl_data.push(Data {
            x: date.to_unix_ms(),
            y: trunc!(quote_pnl / capital * 100.0, 4)
          })
        },
        _ => continue
      }
    }
    let avg_quote_pnl = quote_data.iter().map(|d| d.y).sum::<f64>() / quote_data.len() as f64;
    let avg_pct_pnl = avg_quote_pnl / capital * 100.0;
    let win_rate = (winners as f64 / total_trades as f64) * 100.0;
    let (max_quote, quote_drawdown) = quote_data.iter().fold((0.0, 0.0), |(max, drawdown), d| {
      let max = (max as f64).max(d.y);
      let drawdown = (drawdown as f64).min(d.y - max);
      (max, drawdown)
    });
    let max_pct_drawdown = quote_drawdown / max_quote * 100.0;

    Ok(Summary {
      quote: trunc!(quote, 4),
      pnl: trunc!(quote / capital * 100.0, 4),
      win_rate: trunc!(win_rate, 4),
      total_trades,
      avg_trade_size: self.avg_quote_trade_size()?,
      avg_trade_roi: trunc!(avg_quote_pnl, 4),
      avg_trade_pnl: trunc!(avg_pct_pnl, 4),
      max_pct_drawdown: trunc!(max_pct_drawdown, 4),
      quote_data: Dataset::new(quote_data),
      pnl_data: Dataset::new(pnl_data)
    })
  }

  /// Assumes trading with static position size (e.g. $1000 every trade) and not reinvesting profits.
  pub fn summary(&mut self) -> anyhow::Result<Summary> {
    let mut capital = self.capital;
    let initial_capital = capital;

    let mut quote = 0.0;
    let mut pnl_data = Vec::new();
    let mut quote_data = Vec::new();
    let mut winners = 0;
    let mut total_trades = 0;

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
      let mut quote_pnl = pct_pnl / 100.0 * capital;
      quote_pnl -= quote_pnl.abs() * self.fee / 100.0;

      let quantity = capital / entry.get().price;
      let updated_entry = Trade {
        date: entry.get().date,
        side: entry.get().side,
        quantity,
        price: entry.get().price
      };
      Cell::swap(entry, &Cell::from(updated_entry));

      capital += quote_pnl;
      quote += quote_pnl;

      if quote_pnl > 0.0 {
        winners += 1;
      }
      total_trades += 1;

      quote_data.push(Data {
        x: entry.get().date.to_unix_ms(),
        y: trunc!(quote, 4)
      });
      pnl_data.push(Data {
        x: entry.get().date.to_unix_ms(),
        y: trunc!(quote_pnl / initial_capital * 100.0, 4)
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

    let avg_quote_pnl = quote_data.iter().map(|d| d.y).sum::<f64>() / quote_data.len() as f64;
    let avg_pct_pnl = avg_quote_pnl / initial_capital * 100.0;
    let win_rate = (winners as f64 / total_trades as f64) * 100.0;
    let (max_quote, quote_drawdown) = quote_data.iter().fold((0.0, 0.0), |(max, drawdown), d| {
      let max = (max as f64).max(d.y);
      let drawdown = (drawdown as f64).min(d.y - max);
      (max, drawdown)
    });
    let max_pct_drawdown = quote_drawdown / max_quote * 100.0;

    Ok(Summary {
      quote: trunc!(quote, 4),
      pnl: trunc!((capital - initial_capital) / initial_capital * 100.0, 4),
      win_rate: trunc!(win_rate, 4),
      total_trades,
      avg_trade_size: self.avg_quote_trade_size()?,
      avg_trade_roi: trunc!(avg_quote_pnl, 4),
      avg_trade_pnl: trunc!(avg_pct_pnl, 4),
      max_pct_drawdown: trunc!(max_pct_drawdown, 4),
      quote_data: Dataset::new(quote_data),
      pnl_data: Dataset::new(pnl_data)
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

  pub fn buy_and_hold(
    &mut self,
    op: &Op
  ) -> anyhow::Result<Vec<Data>> {

    let start_capital = self.capital;

    let candles = self.candles.clone();
    let first = candles.first().unwrap();
    let last = candles.last().unwrap();

    let pct_pnl = ((last.close - first.close) / first.close) * 100.0;
    let mut quote_pnl = pct_pnl / 100.0 * start_capital;
    quote_pnl -= quote_pnl.abs() * self.fee / 100.0;

    let data: Dataset = Dataset::new(vec![
      Data {
        x: first.date.to_unix_ms(),
        y: start_capital
      },
      Data {
        x: last.date.to_unix_ms(),
        y: start_capital + quote_pnl
      }
    ]);
    
    Ok(data.translate(op))
  }

  pub fn backtest(
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
          active_trade = Some(trade);
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
            active_trade = Some(trade);
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
async fn sol_backtest() -> anyhow::Result<()> {
  use super::*;
  use time_series::{Day, Month, Plot};
  dotenv::dotenv().ok();

  let period = 10;
  let capital = 1_000.0;
  let fee = 0.15;

  // let start_time = Time::new(2023, &Month::from_num(1), &Day::from_num(1), None, None, None);
  // let end_time = Time::new(2023, &Month::from_num(4), &Day::from_num(22), None, None, None);
  
  let start_time = Time::new(2024, &Month::from_num(4), &Day::from_num(1), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(22), None, None, None);
  

  let out_file = "solusdt_30m.csv";
  let csv = PathBuf::from(out_file);
  let mut backtest = Backtest::new(capital, fee);
  let csv_series = backtest.add_csv_series(&csv, Some(start_time), Some(end_time))?;
  backtest.candles = csv_series.candles;
  backtest.signals = csv_series.signals;

  let k_src = Source::Close;
  let ma_src = Source::Open;
  let k_rev = 0.03;
  let wma_period = 4;

  backtest.backtest(
    wma_period,
    k_rev,
    k_src,
    ma_src
  )?;
  let summary = backtest.summary()?;

  println!("==== Dreamrunner Backtest ====");
  summary.print();

  // let closes: Vec<Data> = backtest.candles.iter().map(|candle| {
  //   Data {
  //     x: candle.date.to_unix_ms(),
  //     y: candle.close
  //   }
  // }).collect();
  // let rust_kagis = backtest.kagis(wma_period, k_rev, k_src, ma_src)?;
  // let pine_kagis = csv_series.kagis;
  // Plot::plot(
  //   vec![closes, rust_kagis, pine_kagis],
  //   "kagi_comparison.png",
  //   "Kagi Comparison",
  //   "Price"
  // )?;

  let strategy = summary.quote_data;
  Plot::plot(
    vec![strategy.0.clone(), backtest.buy_and_hold(&Op::None)?],
    "solusdt_30m_backtest.png",
    "SOL/USDT Dreamrunner Backtest",
    "Equity"
  )?;

  let translated = strategy.translate(&Op::ZScoreMean(period));
  println!("translated len: {}", translated.len());
  if !translated.is_empty() {
    Plot::plot(
      vec![translated],
      "solusdt_30m_translated.png",
      "SOL/USDT Dreamrunner Translated",
      "Z Score"
    )?;
  }

  Ok(())
}

#[tokio::test]
async fn eth_backtest() -> anyhow::Result<()> {
  use super::*;
  use time_series::{Day, Month, Plot};
  dotenv::dotenv().ok();

  let capital = 1_000.0;
  let fee = 0.15;

  let start_time = Time::new(2023, &Month::from_num(1), &Day::from_num(1), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(24), None, None, None);

  let out_file = "ethusdt_30m.csv";
  let csv = PathBuf::from(out_file);
  let mut backtest = Backtest::new(capital, fee);
  let csv_series = backtest.add_csv_series(&csv, Some(start_time), Some(end_time))?;
  backtest.candles = csv_series.candles;
  backtest.signals = csv_series.signals;

  let k_src = Source::Close;
  let ma_src = Source::Open;
  // let k_rev = 10.0;
  // let wma_period = 4;
  let k_rev = 58.4;
  let wma_period = 14;

  backtest.backtest(
    wma_period,
    k_rev,
    k_src,
    ma_src
  )?;
  let summary = backtest.summary()?;

  println!("==== Dreamrunner Backtest ====");
  summary.print();

  Plot::plot(
    vec![summary.quote_data.0, backtest.buy_and_hold(&Op::None)?],
    "ethusdt_30m_backtest.png",
    "ETH/USDT Dreamrunner Backtest",
    "Equity"
  )?;

  Ok(())
}

#[tokio::test]
async fn btc_backtest() -> anyhow::Result<()> {
  use super::*;
  use time_series::{Day, Month, Plot};
  dotenv::dotenv().ok();

  let capital = 1_000.0;
  let fee = 0.15;

  let start_time = Time::new(2023, &Month::from_num(1), &Day::from_num(1), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(24), None, None, None);

  let out_file = "btcusdt_30m.csv";
  let csv = PathBuf::from(out_file);
  let mut backtest = Backtest::new(capital, fee);
  let csv_series = backtest.add_csv_series(&csv, Some(start_time), Some(end_time))?;
  backtest.candles = csv_series.candles;
  backtest.signals = csv_series.signals;

  let k_src = Source::Close;
  let ma_src = Source::Open;
  let k_rev = 58.0; // 1955.0
  let wma_period = 8; // 15

  backtest.backtest(
    wma_period,
    k_rev,
    k_src,
    ma_src
  )?;
  let summary = backtest.summary()?;

  println!("==== Dreamrunner Backtest ====");
  summary.print();

  Plot::plot(
    vec![summary.quote_data.0, backtest.buy_and_hold(&Op::None)?],
    "btcusdt_30m_backtest.png",
    "BTC/USDT Dreamrunner Backtest",
    "Equity"
  )?;

  Ok(())
}

#[tokio::test]
async fn optimize() -> anyhow::Result<()> {
  use super::*;
  use time_series::{Day, Month, Plot};
  use rayon::prelude::{IntoParallelIterator, ParallelIterator};
  dotenv::dotenv().ok();

  // let time_series = "solusdt_30m.csv";
  // let k_rev_start = 0.01;
  // let k_rev_step = 0.01;
  // let out_file = "solusdt_30m_optimal_backtest.png";

  // let time_series = "ethusdt_30m.csv";
  // let k_rev_start = 0.1;
  // let k_rev_step = 0.1;
  // let out_file = "ethusdt_30m_optimal_backtest.png";

  let time_series = "btcusdt_30m.csv";
  let k_rev_start = 1.0;
  let k_rev_step = 1.0;
  let out_file = "btcusdt_30m_optimal_backtest.png";

  let capital = 1_000.0;
  let fee = 0.15;

  let start_time = Time::new(2023, &Month::from_num(1), &Day::from_num(1), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(24), None, None, None);

  let csv = PathBuf::from(time_series);
  let mut backtest = Backtest::new(capital, fee);
  let csv_series = backtest.add_csv_series(&csv, Some(start_time), Some(end_time))?;

  let k_src = Source::Close;
  let ma_src = Source::Open;

  #[derive(Debug, Clone)]
  struct BacktestResult {
    pub k_rev: f64,
    pub wma_period: usize,
    pub summary: Summary
  }

  let mut results: Vec<BacktestResult> = (0..5000).collect::<Vec<usize>>().into_par_iter().flat_map(|i| {
    let k_rev = trunc!(k_rev_start + (i as f64 * k_rev_step), 4);

    let results: Vec<BacktestResult> = (0..15).collect::<Vec<usize>>().into_par_iter().flat_map(|j| {
      let wma_period = j + 1;
      let mut backtest = Backtest::new(capital, fee);
      backtest.candles = csv_series.candles.clone();
      backtest.signals = csv_series.signals.clone();
      backtest.backtest(
        wma_period,
        k_rev,
        k_src,
        ma_src
      )?;
      let summary = backtest.summary()?;
      let res = BacktestResult {
        k_rev,
        wma_period,
        summary
      };
      DreamrunnerResult::<_>::Ok(res)
    }).collect();
    DreamrunnerResult::<_>::Ok(results)
  }).flatten().collect();

  // sort for highest percent ROI first
  results.sort_by(|a, b| b.summary.pnl.partial_cmp(&a.summary.pnl).unwrap());
  let optimized = results.first().unwrap().clone();
  println!("==== Optimized Backtest ====");
  println!("WMA Period: {}", optimized.wma_period);
  println!("Kagi Rev: {}", optimized.k_rev);
  let summary = optimized.summary;

  summary.print();

  Plot::plot(
    vec![summary.quote_data.0],
    out_file,
    "Dreamrunner Backtest",
    "Equity"
  )?;

  Ok(())
}
