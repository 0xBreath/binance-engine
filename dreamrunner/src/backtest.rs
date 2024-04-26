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
        y: 0.0
      },
      Data {
        x: last.date.to_unix_ms(),
        y: quote_pnl
      }
    ]);

    Ok(data.translate(op))
  }

  pub fn backtest(
    &mut self,
    stop_loss: f64,
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

      // check stop loss
      if let Some(trade) = &active_trade {
        let price = candle.close;
        let time = candle.date;
        match trade.side {
          Order::Long => {
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
      let signal = dreamrunner.signal(&mut kagi, &period)?;
      match signal {
        Signal::Long((price, time)) => {
          if let Some(trade) = &active_trade {
            if trade.side == Order::Long {
              continue;
            }
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
        },
        Signal::None => ()
      }
    }

    Ok(())
  }

  /// If compounded, assumes trading profits are 100% reinvested.
  /// If not compounded, assumed trading with fixed capital (e.g. $1000 every trade) and not reinvesting profits.
  pub fn summary(&mut self, compound: bool) -> anyhow::Result<Summary> {
    let mut capital = self.capital;
    let initial_capital = capital;

    let mut quote = 0.0;
    let mut cum_pct = Vec::new();
    let mut cum_quote = Vec::new();
    let mut pct_per_trade = Vec::new();

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
      let mut position_size = match compound {
        true => capital,
        false => initial_capital
      };
      position_size -= position_size.abs() * (self.fee / 100.0); // take exchange fee on trade entry capital
      
      let mut quote_pnl = pct_pnl / 100.0 * position_size;
      quote_pnl -= quote_pnl.abs() * (self.fee / 100.0); // take exchange fee on trade exit capital

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

#[tokio::test]
async fn sol_backtest() -> anyhow::Result<()> {
  use super::*;
  use time_series::{Day, Month, Plot};
  dotenv::dotenv().ok();

  let stop_loss = 2.0;
  let period = 100;
  let capital = 1_000.0;
  let fee = 0.15;
  let compound = false;

  let start_time = Time::new(2023, &Month::from_num(1), &Day::from_num(1), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(22), None, None, None);

  // let start_time = Time::new(2024, &Month::from_num(1), &Day::from_num(1), None, None, None);
  // let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(22), None, None, None);


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
    stop_loss,
    wma_period,
    k_rev,
    k_src,
    ma_src
  )?;
  let summary = backtest.summary(compound)?;

  println!("==== Dreamrunner Backtest ====");
  summary.print();

  Plot::plot(
    // vec![summary.cum_pct.0, backtest.buy_and_hold(&Op::None)?],
    vec![summary.cum_pct.0.clone()],
    "solusdt_30m_backtest.png",
    "SOL/USDT Dreamrunner Backtest",
    "% ROI"
  )?;

  let translated = summary.pct_per_trade.translate(&Op::ZScoreMean(period));
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

  let stop_loss = 5.0;
  let capital = 1_000.0;
  let fee = 0.15;
  let compound = false;

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
    stop_loss,
    wma_period,
    k_rev,
    k_src,
    ma_src
  )?;
  let summary = backtest.summary(compound)?;

  println!("==== Dreamrunner Backtest ====");
  summary.print();

  Plot::plot(
    vec![summary.cum_quote.0, backtest.buy_and_hold(&Op::None)?],
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

  let stop_loss = 5.0;
  let capital = 1_000.0;
  let fee = 0.15;
  let compound = false;

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
    stop_loss,
    wma_period,
    k_rev,
    k_src,
    ma_src
  )?;
  let summary = backtest.summary(compound)?;

  println!("==== Dreamrunner Backtest ====");
  summary.print();

  Plot::plot(
    vec![summary.cum_quote.0, backtest.buy_and_hold(&Op::None)?],
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

  let stop_loss = 5.0;
  let capital = 1_000.0;
  let fee = 0.15;
  let compound = false;

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
        stop_loss,
        wma_period,
        k_rev,
        k_src,
        ma_src
      )?;
      let summary = backtest.summary(compound)?;
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
  results.sort_by(|a, b| b.summary.pct_roi().partial_cmp(&a.summary.pct_roi()).unwrap());

  let optimized = results.first().unwrap().clone();
  println!("==== Optimized Backtest ====");
  println!("WMA Period: {}", optimized.wma_period);
  println!("Kagi Rev: {}", optimized.k_rev);
  let summary = optimized.summary;

  summary.print();

  Plot::plot(
    vec![summary.cum_quote.0],
    out_file,
    "Dreamrunner Backtest",
    "Equity"
  )?;

  Ok(())
}
