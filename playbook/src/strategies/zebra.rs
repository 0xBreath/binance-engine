use log::{info, warn};
use crate::{Strategy};
use time_series::{Candle, Signal, Source, trunc, CandleCache, Data, Dataset, Op};

#[derive(Debug, Clone)]
pub struct Zebra {
  pub src: Source,
  pub period: usize,
  /// Last N candles from current candle.
  /// 0th index is current candle, Nth index is oldest candle.
  pub candles: CandleCache,
  pub threshold: f64
}

impl Zebra {
  pub fn new(src: Source, period: usize, threshold: f64) -> Self {
    Self {
      src,
      period,
      candles: CandleCache::new(period + 1),
      threshold
    }
  }

  pub fn signal(&mut self) -> anyhow::Result<Signal> {
    if self.candles.vec.len() < self.candles.capacity {
      warn!("Insufficient candles to generate signal");
      return Ok(Signal::None);
    }

    // most recent candle is 0th index
    let data = Dataset::new(self.candles.vec.iter().map(|c| {
      Data {
        x: c.date.to_unix_ms(),
        y: match self.src {
          Source::Open => c.open,
          Source::High => c.high,
          Source::Low => c.low,
          Source::Close => c.close
        }
      }
    }).collect());
    let zscores = data.translate(&Op::ZScoreMean(self.period));
    let z_0: f64 = zscores[0].y;
    let z_1: f64 = zscores[1].y;
    let c_0 = &self.candles.vec[0];

    // long if WMA crosses above Kagi and was below Kagi in previous candle
    let long = z_0 > self.threshold && z_1 < self.threshold;
    // short if WMA crosses below Kagi and was above Kagi in previous candle
    let short = z_0 < -(self.threshold) && z_1 > -(self.threshold);

    match (long, short) {
      (true, true) => {
        Err(anyhow::anyhow!("Both long and short signals detected"))
      },
      (true, false) => Ok(Signal::Long((c_0.close, c_0.date))),
      (false, true) => Ok(Signal::Short((c_0.close, c_0.date))),
      (false, false) => Ok(Signal::None)
    }
  }
}

impl Strategy for Zebra {
  /// Appends candle to candle cache and returns a signal (long, short, or do nothing).
  fn process_candle(&mut self, candle: Candle) -> anyhow::Result<Signal> {
    // pushes to front of VecDeque and pops the back if at capacity
    self.candles.push(candle);
    self.signal()
  }

  fn push_candle(&mut self, candle: Candle) {
    self.candles.push(candle);
  }

  fn candles(&self) -> &CandleCache {
    &self.candles
  }
}


// ==========================================================================================
//                                 Dreamrunner Backtests
// ==========================================================================================

#[tokio::test]
async fn sol_backtest() -> anyhow::Result<()> {
  use super::*;
  use std::path::PathBuf;
  use time_series::{Time, Day, Month, Plot};
  use crate::Backtest;
  dotenv::dotenv().ok();

  let src = Source::Close;
  let period = 10;
  let threshold = 2.0;
  let strategy = Zebra::new(src, period, threshold);
  
  // TODO: need minute bars to simulate "ticks" to get more accurate backtest with stop loss
  let stop_loss = 100.0;
  let capital = 1_000.0;
  let fee = 0.02;
  let compound = true;
  let leverage = 1;

  // let start_time = Time::new(2024, &Month::from_num(4), &Day::from_num(24), None, None, None);
  // let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(25), None, None, None);

  let start_time = Time::new(2023, &Month::from_num(1), &Day::from_num(1), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(26), None, None, None);

  let out_file = "solusdt_30m.csv";
  let csv = PathBuf::from(out_file);
  let mut backtest = Backtest::new(strategy, capital, fee, compound, leverage);
  backtest.candles = backtest.add_csv_series(&csv, Some(start_time), Some(end_time))?.candles;

  println!("==== Zebra Backtest ====");
  backtest.backtest(stop_loss)?;
  let summary = backtest.summary()?;
  summary.print();
  Plot::plot(
    vec![summary.cum_pct.0],
    "solusdt_30m_zebra_backtest.png",
    "SOL/USDT Zebra Backtest",
    "% ROI"
  )?;

  Ok(())
}

#[tokio::test]
async fn solusdt_zscore() -> anyhow::Result<()> {
  use super::*;
  use std::path::PathBuf;
  use time_series::{Time, Day, Month, Plot};
  use crate::Backtest;
  dotenv::dotenv().ok();

  let strategy = Dreamrunner::solusdt_optimized();

  let start_time = Time::new(2024, &Month::from_num(4), &Day::from_num(24), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(25), None, None, None);

  // let start_time = Time::new(2023, &Month::from_num(1), &Day::from_num(1), None, None, None);
  // let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(26), None, None, None);

  let out_file = "solusdt_30m.csv";
  let csv = PathBuf::from(out_file);
  let mut backtest = Backtest::new(strategy, 0.0, 0.0, false, 1);
  backtest.candles = backtest.add_csv_series(&csv, Some(start_time), Some(end_time))?.candles;

  let data = Dataset::new(backtest.candles.iter().map(|c| {
    Data {
      x: c.date.to_unix_ms(),
      y: c.close
    }
  }).collect());

  let period = 10;
  let op = Op::ZScoreMean(period);

  Plot::plot(
    vec![data.translate(&op)],
    "solusdt_30m_zscore.png",
    "SOL/USDT Z Score",
    "Z Score"
  )?;

  Ok(())
}