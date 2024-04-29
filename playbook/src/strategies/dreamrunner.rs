use log::{info, warn};
use crate::{Strategy};
use time_series::{Candle, Signal, Source, trunc, CandleCache, Kagi, Data, Dataset, Op};

#[derive(Debug, Clone)]
pub struct Dreamrunner {
  pub k_rev: f64,
  pub k_src: Source,
  pub ma_src: Source,
  pub ma_period: usize,
  /// Last N candles from current candle.
  /// 0th index is current candle, Nth index is oldest candle.
  pub candles: CandleCache,
  pub kagi: Kagi,
}

impl Dreamrunner {
  pub fn new(k_rev: f64, k_src: Source, ma_src: Source, ma_period: usize) -> Self {
    Self {
      k_rev,
      k_src,
      ma_src,
      ma_period,
      candles: CandleCache::new(ma_period + 1),
      kagi: Kagi::default(),
    }
  }

  pub fn solusdt_optimized() -> Self {
    let ma_period = 4;
    Self {
      k_rev: 0.03,
      k_src: Source::Close,
      ma_src: Source::Open,
      ma_period,
      candles: CandleCache::new(ma_period + 1),
      kagi: Kagi::default(),
    }
  }

  pub fn ethusdt_optimized() -> Self {
    let ma_period = 14;
    Self {
      k_rev: 58.4,
      k_src: Source::Close,
      ma_src: Source::Open,
      ma_period,
      candles: CandleCache::new(ma_period + 1),
      kagi: Kagi::default(),
    }
  }

  pub fn btcusdt_optimized() -> Self {
    let ma_period = 8;
    Self {
      k_rev: 58.0,
      k_src: Source::Close,
      ma_src: Source::Open,
      ma_period,
      candles: CandleCache::new(ma_period + 1),
      kagi: Kagi::default(),
    }
  }

  pub fn signal(&mut self) -> anyhow::Result<Signal> {
    if self.candles.vec.len() < 3 {
      warn!("Insufficient candles to generate kagis");
      return Ok(Signal::None);
    }
    if self.candles.vec.len() < self.candles.capacity {
      warn!("Insufficient candles to generate WMA");
      return Ok(Signal::None);
    }

    // prev candle
    let c_1 = self.candles.vec[1];
    // current candle
    let c_0 = self.candles.vec[0];

    // kagi for previous candle
    let k_1 = self.kagi;
    // kagi for current candle
    let k_0 = Kagi::update(&self.kagi, self.k_rev, &c_0, &c_1);
    self.kagi.line = k_0.line;
    self.kagi.direction = k_0.direction;

    let period_1: Vec<&Candle> = self.candles.vec.range(1..self.candles.vec.len()).collect();
    let period_0: Vec<&Candle> = self.candles.vec.range(0..self.candles.vec.len() - 1).collect();

    let wma_1 = self.wma(&period_1);
    let wma_0 = self.wma(&period_0);
    info!("kagi: {}, wma: {}", k_0.line, trunc!(wma_0, 2));

    // long if WMA crosses above Kagi and was below Kagi in previous candle
    let long = wma_0 > k_0.line && wma_1 < k_1.line;
    // short if WMA crosses below Kagi and was above Kagi in previous candle
    let short = wma_0 < k_0.line && wma_1 > k_1.line;

    match (long, short) {
      (true, true) => {
        Err(anyhow::anyhow!("Both long and short signals detected"))
      },
      (true, false) => Ok(Signal::Long((c_0.close, c_0.date))),
      (false, true) => Ok(Signal::Short((c_0.close, c_0.date))),
      (false, false) => Ok(Signal::None)
    }
  }

  pub fn wma(&self, candles: &[&Candle]) -> f64 {
    let mut norm = 0.0;
    let mut sum = 0.0;
    let len = candles.len();
    for (i,  c) in candles.iter().enumerate() {
      let weight = ((len - i) * len) as f64;
      let src = match self.ma_src {
        Source::Open => c.open,
        Source::High => c.high,
        Source::Low => c.low,
        Source::Close => c.close
      };
      norm += weight;
      sum += src * weight;
    }
    sum / norm
  }
}

impl Strategy for Dreamrunner {
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

  let strategy = Dreamrunner::solusdt_optimized();
  let stop_loss = 100.0;
  let capital = 1_000.0;
  let fee = 0.01;
  let compound = true;
  let leverage = 1;

  // let start_time = Time::new(2024, &Month::from_num(4), &Day::from_num(20), None, None, None);
  // let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(25), None, None, None);

  let start_time = Time::new(2023, &Month::from_num(1), &Day::from_num(1), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(26), None, None, None);

  let out_file = "solusdt_30m.csv";
  let csv = PathBuf::from(out_file);
  let mut backtest = Backtest::new(strategy.clone(), capital, fee, compound, leverage);
  backtest.candles = backtest.add_csv_series(&csv, Some(start_time), Some(end_time))?.candles;

  println!("==== Dreamrunner Backtest ====");
  backtest.backtest(stop_loss)?;
  let summary = backtest.summary()?;
  summary.print();
  Plot::plot(
    vec![summary.cum_pct.0, backtest.buy_and_hold(&Op::None)?],
    "solusdt_30m_dreamrunner_backtest.png",
    "SOL/USDT Dreamrunner Backtest",
    "% ROI"
  )?;


  // ==== Dreamrunner Strategy ====
  let mut strategy_kagis = vec![];
  let mut strategy_wmas = vec![];
  let mut strategy_signals = vec![];

  let mut strategy = strategy.clone();
  let mut backtest = Backtest::new(strategy.clone(), capital, fee, compound, leverage);
  let csv_series = backtest.add_csv_series(&csv, Some(start_time), Some(end_time))?;
  backtest.candles = csv_series.candles;
  backtest.signals = csv_series.signals;
  let candles = backtest.candles.clone();
  for candle in candles {
    let signal = strategy.process_candle(candle)?;

    match signal {
      Signal::Long((price, date)) => {
        strategy_signals.push(Data {
          x: date.to_unix_ms(),
          y: price
        });
      }
      Signal::Short((price, date)) => {
        strategy_signals.push(Data {
          x: date.to_unix_ms(),
          y: price
        });
      }
      Signal::None => ()
    }

    strategy_kagis.push(Data {
      x: candle.date.to_unix_ms(),
      y: strategy.kagi.line
    });
    let cached_candles: Vec<&Candle> = strategy.candles.vec.range(0..strategy.candles.vec.len() - 1).collect();
    strategy_wmas.push(Data {
      x: candle.date.to_unix_ms(),
      y: strategy.wma(&cached_candles)
    });
  }
  // remove first indices
  strategy_kagis = strategy_kagis.into_iter().skip(4).collect();
  strategy_wmas = strategy_wmas.into_iter().skip(4).collect();

  let closes = Dataset::new(backtest.candles.iter().map(|c| {
    Data {
      x: c.date.to_unix_ms(),
      y: c.close
    }
  }).collect());
  Plot::plot(
    vec![strategy_kagis, strategy_wmas, closes.0],
    "solusdt_30m_dreamrunner_strategy.png",
    "Dreamrunner Strategy",
    "Price"
  )?;

  Ok(())
}

#[tokio::test]
async fn eth_backtest() -> anyhow::Result<()> {
  use super::*;
  use std::path::PathBuf;
  use time_series::{Time, Day, Month, Plot, Op};
  use crate::Backtest;
  dotenv::dotenv().ok();

  let strategy = Dreamrunner::ethusdt_optimized();
  let stop_loss = 5.0;
  let capital = 1_000.0;
  let fee = 0.15;
  let compound = false;
  let leverage = 1;

  let start_time = Time::new(2023, &Month::from_num(1), &Day::from_num(1), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(24), None, None, None);

  let out_file = "ethusdt_30m.csv";
  let csv = PathBuf::from(out_file);
  let mut backtest = Backtest::new(strategy, capital, fee, compound, leverage);
  let csv_series = backtest.add_csv_series(&csv, Some(start_time), Some(end_time))?;
  backtest.candles = csv_series.candles;
  backtest.signals = csv_series.signals;

  backtest.backtest(stop_loss)?;
  let summary = backtest.summary()?;

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
  use std::path::PathBuf;
  use time_series::{Time, Day, Month, Plot, Op};
  use crate::Backtest;
  dotenv::dotenv().ok();

  let strategy = Dreamrunner::btcusdt_optimized();
  let stop_loss = 5.0;
  let capital = 1_000.0;
  let fee = 0.15;
  let compound = false;
  let leverage = 1;

  let start_time = Time::new(2023, &Month::from_num(1), &Day::from_num(1), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(24), None, None, None);

  let out_file = "btcusdt_30m.csv";
  let csv = PathBuf::from(out_file);
  let mut backtest = Backtest::new(strategy, capital, fee, compound, leverage);
  let csv_series = backtest.add_csv_series(&csv, Some(start_time), Some(end_time))?;
  backtest.candles = csv_series.candles;
  backtest.signals = csv_series.signals;

  backtest.backtest(stop_loss)?;
  let summary = backtest.summary()?;

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
  use std::path::PathBuf;
  use time_series::{Time, Day, Month, Plot, Summary};
  use crate::Backtest;
  use rayon::prelude::{IntoParallelIterator, ParallelIterator};
  dotenv::dotenv().ok();

  let stop_loss = 100.0;
  let capital = 1_000.0;
  let fee = 0.01;
  let compound = true;
  let leverage = 1;

  let strategy = Dreamrunner::solusdt_optimized();
  let time_series = "solusdt_30m.csv";
  let k_rev_start = 0.02;
  let k_rev_step = 0.01;
  let out_file = "solusdt_30m_optimal_backtest.png";

  // let strategy = Dreamrunner::ethusdt_optimized();
  // let time_series = "ethusdt_30m.csv";
  // let k_rev_start = 0.1;
  // let k_rev_step = 0.1;
  // let out_file = "ethusdt_30m_optimal_backtest.png";

  // let strategy = Dreamrunner::btcusdt_optimized();
  // let time_series = "btcusdt_30m.csv";
  // let k_rev_start = 1.0;
  // let k_rev_step = 1.0;
  // let out_file = "btcusdt_30m_optimal_backtest.png";

  let start_time = Time::new(2023, &Month::from_num(1), &Day::from_num(1), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(26), None, None, None);

  let csv = PathBuf::from(time_series);
  let mut backtest = Backtest::new(strategy.clone(), capital, fee, compound, leverage);
  let csv_series = backtest.add_csv_series(&csv, Some(start_time), Some(end_time))?;

  #[derive(Debug, Clone)]
  struct BacktestResult {
    pub k_rev: f64,
    pub wma_period: usize,
    pub summary: Summary
  }

  let mut results: Vec<BacktestResult> = (0..1_000).collect::<Vec<usize>>().into_par_iter().flat_map(|i| {
    let k_rev = trunc!(k_rev_start + (i as f64 * k_rev_step), 4);

    let results: Vec<BacktestResult> = (0..20).collect::<Vec<usize>>().into_par_iter().flat_map(|j| {
      let wma_period = j + 1;
      let mut backtest = Backtest::new(strategy.clone(), capital, fee, compound, leverage);
      backtest.candles = csv_series.candles.clone();
      backtest.signals = csv_series.signals.clone();
      backtest.backtest(stop_loss)?;
      let summary = backtest.summary()?;
      let res = BacktestResult {
        k_rev,
        wma_period,
        summary
      };
      Result::<_, anyhow::Error>::Ok(res)
    }).collect();
    Result::<_, anyhow::Error>::Ok(results)
  }).flatten().collect();

  // sort for highest percent ROI first
  results.sort_by(|a, b| b.summary.pct_roi().partial_cmp(&a.summary.pct_roi()).unwrap());

  let optimized = results.first().unwrap().clone();
  println!("==== Optimized Backtest ====");
  println!("WMA Period: {}", optimized.wma_period);
  println!("Kagi Rev: {}", optimized.k_rev);
  let summary = optimized.summary;
  summary.print();
  
  backtest.candles = csv_series.candles;
  Plot::plot(
    vec![summary.cum_pct.0, backtest.buy_and_hold(&Op::None)?],
    out_file,
    "Dreamrunner Optimal Backtest",
    "% ROI"
  )?;

  Ok(())
}

// Tradingview versus Dreamrunner
#[tokio::test]
async fn pine_versus_rust() -> anyhow::Result<()> {
  use super::*;
  use std::path::PathBuf;
  use time_series::{Time, Day, Month, Plot, Data, Candle, Signal};
  use crate::Backtest;
  dotenv::dotenv().ok();

  let capital = 1_000.0;
  let fee = 0.0;
  let compound = false;
  let leverage = 1;

  let start_time = Time::new(2024, &Month::from_num(3), &Day::from_num(20), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(3), &Day::from_num(22), None, None, None);

  // let start_time = Time::new(2023, &Month::from_num(1), &Day::from_num(1), None, None, None);
  // let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(26), None, None, None);

  let out_file = "solusdt_30m_repaint.csv";
  let csv = PathBuf::from(out_file);

  let mut strategy_kagis = vec![];
  let mut strategy_wmas = vec![];
  let mut strategy_signals = vec![];

  let mut strategy = Dreamrunner::solusdt_optimized();
  let mut backtest = Backtest::new(strategy.clone(), capital, fee, compound, leverage);
  let csv_series = backtest.add_csv_series(&csv, Some(start_time), Some(end_time))?;
  backtest.candles = csv_series.candles;
  backtest.signals = csv_series.signals;
  let candles = backtest.candles.clone();
  for candle in candles {
    let signal = strategy.process_candle(candle)?;

    match signal {
      Signal::Long((price, date)) => {
        strategy_signals.push(Data {
          x: date.to_unix_ms(),
          y: price
        });
      }
      Signal::Short((price, date)) => {
        strategy_signals.push(Data {
          x: date.to_unix_ms(),
          y: price
        });
      }
      Signal::None => ()
    }

    strategy_kagis.push(Data {
      x: candle.date.to_unix_ms(),
      y: strategy.kagi.line
    });
    let cached_candles: Vec<&Candle> = strategy.candles.vec.range(0..strategy.candles.vec.len() - 1).collect();
    strategy_wmas.push(Data {
      x: candle.date.to_unix_ms(),
      y: strategy.wma(&cached_candles)
    });
  }
  // remove first 4 indices
  // strategy_kagis.retain(
  //   |c| c.y > 0.0
  // );
  // strategy_wmas.retain(
  //   |c| c.y > 0.0
  // );
  strategy_kagis = strategy_kagis.into_iter().skip(10).collect();
  strategy_wmas = strategy_wmas.into_iter().skip(10).collect();
  
  let closes = Dataset::new(backtest.candles.iter().map(|c| {
    Data {
      x: c.date.to_unix_ms(),
      y: c.close
    }
  }).collect());

  Plot::plot(
    // vec![closes.data().clone(), strategy_kagis, strategy_wmas, strategy_signals],
    vec![closes.data().clone(), strategy_kagis],
    "rust.png",
    "Rust",
    "Price"
  )?;
  // closes = cyan
  // kagis = red
  // wmas = lime

  let pine_signals: Vec<Data> = backtest.signals.iter().flat_map(|s| {
    match s {
      Signal::Long((price, date)) => {
        Some(Data {
          x: date.to_unix_ms(),
          y: 140.0 //*price
        })
      }
      Signal::Short((price, date)) => {
        Some(Data {
          x: date.to_unix_ms(),
          y: 144.0 //*price
        })
      }
      Signal::None => None
    }
  }).collect();
  Plot::plot(
    // vec![csv_series.kagis, csv_series.wmas],
    vec![closes.data().clone(), csv_series.kagis],
    "pine.png",
    "Pine",
    "Price"
  )?;

  let pine_backtest = backtest.backtest_tradingview(compound)?;
  Plot::plot(
    vec![pine_backtest.cum_pct.0],
    "pine_backtest.png",
    "Tradingview Backtest",
    "Price"
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