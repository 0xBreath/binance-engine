use log::{info, warn};
use crate::{Strategy};
use time_series::{Candle, Signal, Source, trunc, DataCache, Kagi, SignalInfo};

#[derive(Debug, Clone)]
pub struct Dreamrunner {
  pub ticker: String,
  pub k_rev: f64,
  pub k_src: Source,
  pub ma_src: Source,
  pub ma_period: usize,
  /// Last N candles from current candle.
  /// 0th index is current candle, Nth index is oldest candle.
  pub candles: DataCache<Candle>,
  pub kagi: Kagi,
}

impl Dreamrunner {
  pub fn new(ticker: String, k_rev: f64, k_src: Source, ma_src: Source, ma_period: usize) -> Self {
    Self {
      ticker: ticker.clone(),
      k_rev,
      k_src,
      ma_src,
      ma_period,
      candles: DataCache::new(ma_period + 1, ticker),
      kagi: Kagi::default(),
    }
  }

  pub fn solusdt_optimized() -> Self {
    let ma_period = 4;
    Self {
      ticker: "SOLUSDT".to_string(),
      k_rev: 0.03,
      k_src: Source::Close,
      ma_src: Source::Open,
      ma_period,
      candles: DataCache::new(ma_period + 1, "SOLUSDT".to_string()),
      kagi: Kagi::default(),
    }
  }

  pub fn ethusdt_optimized() -> Self {
    let ma_period = 14;
    Self {
      ticker: "ETHUSDT".to_string(),
      k_rev: 58.4,
      k_src: Source::Close,
      ma_src: Source::Open,
      ma_period,
      candles: DataCache::new(ma_period + 1, "ETHUSDT".to_string()),
      kagi: Kagi::default(),
    }
  }

  pub fn btcusdt_optimized() -> Self {
    let ma_period = 8;
    Self {
      ticker: "BTCUSDT".to_string(),
      k_rev: 58.0,
      k_src: Source::Close,
      ma_src: Source::Open,
      ma_period,
      candles: DataCache::new(ma_period + 1, "BTCUSDT".to_string()),
      kagi: Kagi::default(),
    }
  }

  pub fn signal(&mut self) -> anyhow::Result<Vec<Signal>> {
    if self.candles.vec.len() < 3 {
      warn!("Insufficient candles to generate kagis");
      return Ok(vec![]);
    }
    if self.candles.vec.len() < self.candles.capacity {
      warn!("Insufficient candles to generate WMA");
      return Ok(vec![]);
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
    let enter_long = wma_0 > k_0.line && wma_1 < k_1.line;
    // short if WMA crosses below Kagi and was above Kagi in previous candle
    let exit_long = wma_0 < k_0.line && wma_1 > k_1.line;
    let enter_short = exit_long;
    let exit_short = enter_long;
    
    let info = SignalInfo {
      price: c_0.close,
      date: c_0.date,
      ticker: self.ticker.clone()
    };
    
    let mut signals = vec![];
    
    // process exits before any new entries
    if exit_long {
      signals.push(Signal::ExitLong(info.clone()));
    }
    if exit_short {
      signals.push(Signal::ExitShort(info.clone()));
    }
    if enter_long {
      signals.push(Signal::EnterLong(info.clone()));
    }
    if enter_short {
      signals.push(Signal::EnterShort(info));
    }
    Ok(signals)
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

impl Strategy<Candle> for Dreamrunner {
  /// Appends candle to candle cache and returns a signal (long, short, or do nothing).
  fn process_candle(&mut self, candle: Candle, _ticker: Option<String>) -> anyhow::Result<Vec<Signal>> {
    self.candles.push(candle);
    self.signal()
  }

  fn push_candle(&mut self, candle: Candle, _ticker: Option<String>) {
    self.candles.push(candle);
  }

  fn cache(&self, _ticker: Option<String>) -> Option<&DataCache<Candle>> {
    Some(&self.candles)
  }
}


// ==========================================================================================
//                                 Dreamrunner Backtests
// ==========================================================================================

#[tokio::test]
async fn dreamrunner_sol() -> anyhow::Result<()> {
  use super::*;
  use std::path::PathBuf;
  use time_series::{Time, Day, Month, Plot};
  use crate::Backtest;
  dotenv::dotenv().ok();

  let strategy = Dreamrunner::solusdt_optimized();
  // todo: tiny stop loss makes astronomical returns, but is this realistic?
  let stop_loss = 0.1;
  let capital = 1_000.0;
  let fee = 0.02;
  let compound = true;
  let leverage = 1;
  let short_selling = false;
  let ticker = "SOLUSDT".to_string();

  // let start_time = Time::new(2024, &Month::from_num(4), &Day::from_num(28), None, None, None);
  // let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(30), Some(10), Some(25), None);

  let start_time = Time::new(2023, &Month::from_num(1), &Day::from_num(1), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(30), None, None, None);

  let out_file = "solusdt_30m.csv";
  let csv = PathBuf::from(out_file);
  let mut backtest = Backtest::new(strategy.clone(), capital, fee, compound, leverage, short_selling);
  let csv_series = Backtest::<Candle, Dreamrunner>::csv_series(&csv, Some(start_time), Some(end_time), ticker.clone())?;
  backtest.candles.insert(ticker.clone(), csv_series.candles);

  println!("==== Dreamrunner Backtest ====");
  let summary = backtest.backtest(stop_loss)?;
  let all_buy_and_hold = backtest.buy_and_hold()?;
  let buy_and_hold = all_buy_and_hold
    .get(&ticker)
    .ok_or(anyhow::anyhow!("Buy and hold not found for ticker"))?
    .clone();
  summary.print(&ticker);
  Plot::plot(
    vec![summary.cum_pct(&ticker)?.data().clone(), buy_and_hold],
    "dreamrunner_sol_backtest.png",
    "SOL/USDT Dreamrunner Backtest",
    "% ROI",
    "Unix Millis"
  )?;

  Ok(())
}

#[tokio::test]
async fn eth_backtest() -> anyhow::Result<()> {
  use super::*;
  use std::path::PathBuf;
  use time_series::{Time, Day, Month, Plot};
  use crate::Backtest;
  dotenv::dotenv().ok();

  let strategy = Dreamrunner::ethusdt_optimized();
  let stop_loss = 5.0;
  let capital = 1_000.0;
  let fee = 0.15;
  let compound = false;
  let leverage = 1;
  let short_selling = false;
  let ticker = "ETHUSDT".to_string();

  let start_time = Time::new(2023, &Month::from_num(1), &Day::from_num(1), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(24), None, None, None);

  let out_file = "ethusdt_30m.csv";
  let csv = PathBuf::from(out_file);
  let mut backtest = Backtest::new(strategy, capital, fee, compound, leverage, short_selling);
  let csv_series = Backtest::<Candle, Dreamrunner>::csv_series(&csv, Some(start_time), Some(end_time), ticker.clone())?;
  backtest.candles.insert(ticker.clone(), csv_series.candles);

  let summary = backtest.backtest(stop_loss)?;

  println!("==== Dreamrunner Backtest ====");
  summary.print(&ticker);
  let all_buy_and_hold = backtest.buy_and_hold()?;
  let buy_and_hold = all_buy_and_hold
    .get(&ticker)
    .ok_or(anyhow::anyhow!("Buy and hold not found for ticker"))?
    .clone();
  Plot::plot(
    vec![summary.cum_quote(&ticker)?.data().clone(), buy_and_hold],
    "ethusdt_30m_backtest.png",
    "ETH/USDT Dreamrunner Backtest",
    "Equity",
    "Unix Millis"
  )?;

  Ok(())
}

#[tokio::test]
async fn btc_backtest() -> anyhow::Result<()> {
  use std::path::PathBuf;
  use time_series::{Time, Day, Month, Plot};
  use crate::Backtest;
  dotenv::dotenv().ok();

  let strategy = Dreamrunner::btcusdt_optimized();
  let stop_loss = 5.0;
  let capital = 1_000.0;
  let fee = 0.15;
  let compound = false;
  let leverage = 1;
  let short_selling = false;
  let ticker = "BTCUSDT".to_string();

  let start_time = Time::new(2023, &Month::from_num(1), &Day::from_num(1), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(24), None, None, None);

  let out_file = "btcusdt_30m.csv";
  let csv = PathBuf::from(out_file);
  let mut backtest = Backtest::new(strategy, capital, fee, compound, leverage, short_selling);
  let csv_series = Backtest::<Candle, Dreamrunner>::csv_series(&csv, Some(start_time), Some(end_time), ticker.clone())?;
  backtest.candles.insert(ticker.clone(), csv_series.candles);

  let summary = backtest.backtest(stop_loss)?;

  println!("==== Dreamrunner Backtest ====");
  summary.print(&ticker);
  let all_buy_and_hold = backtest.buy_and_hold()?;
  let buy_and_hold = all_buy_and_hold
    .get(&ticker)
    .ok_or(anyhow::anyhow!("Buy and hold not found for ticker"))?
    .clone();
  Plot::plot(
    vec![summary.cum_quote(&ticker)?.data().clone(), buy_and_hold],
    "btcusdt_30m_backtest.png",
    "BTC/USDT Dreamrunner Backtest",
    "Equity",
    "Unix Millis"
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
  let short_selling = false;

  let strategy = Dreamrunner::solusdt_optimized();
  let time_series = "solusdt_30m.csv";
  let k_rev_start = 0.02;
  let k_rev_step = 0.01;
  let out_file = "solusdt_30m_optimal_backtest.png";
  let ticker = "SOLUSDT".to_string();

  // let strategy = Dreamrunner::ethusdt_optimized();
  // let time_series = "ethusdt_30m.csv";
  // let k_rev_start = 0.1;
  // let k_rev_step = 0.1;
  // let out_file = "ethusdt_30m_optimal_backtest.png";
  // let ticker = "ETHUSDT".to_string();

  // let strategy = Dreamrunner::btcusdt_optimized();
  // let time_series = "btcusdt_30m.csv";
  // let k_rev_start = 1.0;
  // let k_rev_step = 1.0;
  // let out_file = "btcusdt_30m_optimal_backtest.png";
  // let ticker = "BTCUSDT".to_string();

  let start_time = Time::new(2023, &Month::from_num(1), &Day::from_num(1), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(26), None, None, None);

  let csv = PathBuf::from(time_series);
  let mut backtest = Backtest::new(strategy.clone(), capital, fee, compound, leverage, short_selling);
  let csv_series = Backtest::<Candle, Dreamrunner>::csv_series(&csv, Some(start_time), Some(end_time), ticker.clone())?;

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
      let mut backtest = Backtest::new(strategy.clone(), capital, fee, compound, leverage, short_selling);
      backtest.candles.insert(ticker.clone(), csv_series.candles.clone());
      let summary = backtest.backtest(stop_loss)?;
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
  results.sort_by(|a, b| {
    b.summary.pct_roi(&ticker).partial_cmp(&a.summary.pct_roi(&ticker)).unwrap()
  });

  let optimized = results.first().unwrap().clone();
  println!("==== Optimized Backtest ====");
  println!("WMA Period: {}", optimized.wma_period);
  println!("Kagi Rev: {}", optimized.k_rev);
  let summary = optimized.summary;
  summary.print(&ticker);
  let all_buy_and_hold = backtest.buy_and_hold()?;
  let buy_and_hold = all_buy_and_hold
    .get(&ticker)
    .ok_or(anyhow::anyhow!("Buy and hold not found for ticker"))?
    .clone();
  backtest.candles.insert(ticker.clone(), csv_series.candles);
  Plot::plot(
    vec![summary.cum_pct(&ticker)?.data().clone(), buy_and_hold],
    out_file,
    "Dreamrunner Optimal Backtest",
    "% ROI",
    "Unix Millis"
  )?;

  Ok(())
}