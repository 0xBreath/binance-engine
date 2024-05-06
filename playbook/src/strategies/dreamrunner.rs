#![allow(unused_imports)]

use std::path::PathBuf;
use log::{info, warn};
use crate::Strategy;
use time_series::*;
use rayon::prelude::*;
use crate::Backtest;

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
  pub stop_loss_pct: Option<f64>
}

impl Dreamrunner {
  pub fn new(ticker: String, k_rev: f64, k_src: Source, ma_src: Source, ma_period: usize, stop_loss_pct: Option<f64>) -> Self {
    Self {
      ticker: ticker.clone(),
      k_rev,
      k_src,
      ma_src,
      ma_period,
      candles: DataCache::new(ma_period + 1, ticker),
      kagi: Kagi::default(),
      stop_loss_pct
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
      stop_loss_pct: Some(1.0)
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
      stop_loss_pct: Some(100.0)
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
      stop_loss_pct: Some(1.0)
    }
  }
  pub fn btcusd_1d_optimized(stop_loss_pct: Option<f64>) -> Self {
    let ma_period = 8;
    Self {
      ticker: "BTCUSD".to_string(),
      k_rev: 58.0,
      k_src: Source::Close,
      ma_src: Source::Open,
      ma_period,
      candles: DataCache::new(ma_period + 1, "BTCUSD".to_string()),
      kagi: Kagi::default(),
      stop_loss_pct
    }
  }
  pub fn atlasusd_1h_optimized(stop_loss_pct: Option<f64>) -> Self {
    let ma_period = 10; // 2
    Self {
      ticker: "ATLASUSD".to_string(),
      k_rev: 0.00001, //0.00006,
      k_src: Source::Close,
      ma_src: Source::Open,
      ma_period,
      candles: DataCache::new(ma_period + 1, "ATLASUSD".to_string()),
      kagi: Kagi::default(),
      stop_loss_pct
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
  
  fn stop_loss_pct(&self) -> Option<f64> {
    self.stop_loss_pct
  }
}


// ==========================================================================================
//                                 Dreamrunner Backtests
// ==========================================================================================

#[tokio::test]
async fn dreamrunner_sol() -> anyhow::Result<()> {
  use super::*;
  dotenv::dotenv().ok();

  let strategy = Dreamrunner::solusdt_optimized();
  let capital = 1_000.0;
  let fee = 0.02;
  let bet = Bet::Percent(100.0);
  let leverage = 1;
  let short_selling = true;
  let ticker = "SOLUSDT".to_string();

  let start_time = Time::new(2023, &Month::from_num(1), &Day::from_num(1), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(30), None, None, None);

  let out_file = "solusdt_30m.csv";
  let csv = PathBuf::from(out_file);
  let mut backtest = Backtest::new(strategy.clone(), capital, fee, bet, leverage, short_selling);
  let csv_series = Dataframe::csv_series(&csv, Some(start_time), Some(end_time), ticker.clone())?;
  backtest.candles.insert(ticker.clone(), csv_series.candles);

  let summary = backtest.backtest()?;
  let all_buy_and_hold = backtest.buy_and_hold()?;
  let buy_and_hold = all_buy_and_hold
    .get(&ticker)
    .ok_or(anyhow::anyhow!("Buy and hold not found for ticker"))?
    .clone();
  summary.print(&ticker);
  Plot::plot(
    vec![summary.cum_pct(&ticker)?.data().clone(), buy_and_hold],
    "dreamrunner_sol_30m_backtest.png",
    "SOL/USDT Dreamrunner Backtest",
    "% ROI",
    "Unix Millis"
  )?;

  Ok(())
}

#[tokio::test]
async fn eth_backtest() -> anyhow::Result<()> {
  use super::*;
  dotenv::dotenv().ok();

  let strategy = Dreamrunner::ethusdt_optimized();
  let capital = 1_000.0;
  let fee = 0.15;
  let bet = Bet::Static;
  let leverage = 1;
  let short_selling = false;
  let ticker = "ETHUSDT".to_string();

  let start_time = Time::new(2023, &Month::from_num(1), &Day::from_num(1), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(24), None, None, None);

  let out_file = "ethusdt_30m.csv";
  let csv = PathBuf::from(out_file);
  let mut backtest = Backtest::new(strategy, capital, fee, bet, leverage, short_selling);
  let csv_series = Dataframe::csv_series(&csv, Some(start_time), Some(end_time), ticker.clone())?;
  backtest.candles.insert(ticker.clone(), csv_series.candles);

  let summary = backtest.backtest()?;

  println!("==== Dreamrunner Backtest ====");
  summary.print(&ticker);
  let all_buy_and_hold = backtest.buy_and_hold()?;
  let buy_and_hold = all_buy_and_hold
    .get(&ticker)
    .ok_or(anyhow::anyhow!("Buy and hold not found for ticker"))?
    .clone();
  Plot::plot(
    vec![summary.cum_pct(&ticker)?.data().clone(), buy_and_hold],
    "ethusdt_30m_backtest.png",
    &format!("{} Dreamrunner Backtest", ticker),
    "Equity",
    "Unix Millis"
  )?;

  Ok(())
}

#[tokio::test]
async fn btc_1d_backtest() -> anyhow::Result<()> {
  use super::*;
  dotenv::dotenv().ok();
  
  let stop_loss = 5.0;
  let capital = 1_000.0;
  let fee = 0.02;
  let bet = Bet::Percent(100.0);
  let leverage = 1;
  let short_selling = true;
  let ticker = "BTCUSD".to_string();
  let out_file = "btcusd_1d.csv";
  let strategy = Dreamrunner::new(
    ticker.clone(),
    58.0,
    Source::Close,
    Source::Open,
    8,
    Some(stop_loss)
  );

  let start_time = Time::new(2012, &Month::from_num(1), &Day::from_num(1), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(5), &Day::from_num(1), None, None, None);

  let csv = PathBuf::from(out_file);
  let mut backtest = Backtest::new(strategy, capital, fee, bet, leverage, short_selling);
  let csv_series = Dataframe::csv_series(&csv, Some(start_time), Some(end_time), ticker.clone())?;
  backtest.candles.insert(ticker.clone(), csv_series.candles);

  let summary = backtest.backtest()?;
  
  summary.print(&ticker);
  let all_buy_and_hold = backtest.buy_and_hold()?;
  let buy_and_hold = all_buy_and_hold
    .get(&ticker)
    .ok_or(anyhow::anyhow!("Buy and hold not found for ticker"))?
    .clone();
  Plot::plot(
    vec![summary.cum_pct(&ticker)?.data().clone(), buy_and_hold],
    "dreamrunner_btc_1d_backtest.png",
    &format!("{} Dreamrunner Backtest", ticker),
    "Equity",
    "Unix Millis"
  )?;

  Ok(())
}

#[tokio::test]
async fn btc_30m_backtest() -> anyhow::Result<()> {
  use super::*;
  dotenv::dotenv().ok();

  let strategy = Dreamrunner::btcusdt_optimized();
  let capital = 1_000.0;
  let fee = 0.02;
  let bet = Bet::Percent(90.0);
  let leverage = 1;
  let short_selling = false;
  let ticker = "BTCUSDT".to_string();
  let out_file = "btcusdt_30m.csv";

  let start_time = Time::new(2023, &Month::from_num(1), &Day::from_num(1), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(24), None, None, None);
  
  let csv = PathBuf::from(out_file);
  let mut backtest = Backtest::new(strategy, capital, fee, bet, leverage, short_selling);
  let csv_series = Dataframe::csv_series(&csv, Some(start_time), Some(end_time), ticker.clone())?;
  backtest.candles.insert(ticker.clone(), csv_series.candles);

  let summary = backtest.backtest()?;

  summary.print(&ticker);
  let all_buy_and_hold = backtest.buy_and_hold()?;
  let buy_and_hold = all_buy_and_hold
    .get(&ticker)
    .ok_or(anyhow::anyhow!("Buy and hold not found for ticker"))?
    .clone();
  Plot::plot(
    vec![summary.cum_pct(&ticker)?.data().clone(), buy_and_hold],
    "dreamrunner_btc_30m_backtest.png",
    &format!("{} Dreamrunner Backtest", ticker),
    "Equity",
    "Unix Millis"
  )?;

  Ok(())
}

#[tokio::test]
async fn atlas_1h_backtest() -> anyhow::Result<()> {
  use super::*;
  dotenv::dotenv().ok();

  let stop_loss = 1.0;
  let capital = 1_000.0;
  let fee = 0.02;
  let bet = Bet::Percent(90.0);
  let leverage = 1;
  let short_selling = false;
  let strategy = Dreamrunner::atlasusd_1h_optimized(Some(stop_loss));

  let time_series = "atlasusd_1h.csv";
  let out_file = "dreamrunner_atlas_1h_backtest.png";
  let ticker = "ATLASUSD".to_string();

  let start_time = Time::new(2022, &Month::from_num(1), &Day::from_num(24), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(5), &Day::from_num(1), None, None, None);

  // let start_time = Time::new(2022, &Month::from_num(1), &Day::from_num(24), None, None, None);
  // let end_time = Time::new(2022, &Month::from_num(12), &Day::from_num(5), None, None, None);

  let csv = PathBuf::from(time_series);
  let mut backtest = Backtest::new(strategy, capital, fee, bet, leverage, short_selling);
  let csv_series = Dataframe::csv_series(&csv, Some(start_time), Some(end_time), ticker.clone())?;
  backtest.candles.insert(ticker.clone(), csv_series.candles);

  let summary = backtest.backtest()?;

  summary.print(&ticker);
  let all_buy_and_hold = backtest.buy_and_hold()?;
  let buy_and_hold = all_buy_and_hold
    .get(&ticker)
    .ok_or(anyhow::anyhow!("Buy and hold not found for ticker"))?
    .clone();
  Plot::plot(
    vec![summary.cum_pct(&ticker)?.data().clone(), buy_and_hold],
    out_file,
    &format!("{} Dreamrunner Backtest", ticker),
    "% ROI",
    "Unix Millis"
  )?;

  Ok(())
}

#[tokio::test]
async fn optimize() -> anyhow::Result<()> {
  use super::*;
  dotenv::dotenv().ok();

  // let strategy = Dreamrunner::solusdt_optimized();
  // let time_series = "solusdt_30m.csv";
  // let k_rev_start = 0.02;
  // let k_rev_step = 0.01;
  // let out_file = "dreamrunner_sol_30m_optimal_backtest.png";
  // let ticker = "SOLUSDT".to_string();

  // let strategy = Dreamrunner::ethusdt_optimized();
  // let time_series = "ethusdt_30m.csv";
  // let k_rev_start = 0.1;
  // let k_rev_step = 0.1;
  // let out_file = "dreamrunner_eth_30m_optimal_backtest.png";
  // let ticker = "ETHUSDT".to_string();

  // let strategy = Dreamrunner::btcusdt_optimized();
  // let time_series = "btcusdt_30m.csv";
  // let k_rev_start = 1.0;
  // let k_rev_step = 1.0;
  // let out_file = "dreamrunner_btc_30m_optimal_backtest.png";
  // let ticker = "BTCUSDT".to_string();

  // let strategy = Dreamrunner::btcusd_1d_optimized(Some(stop_loss));
  // let time_series = "btcusd_1d.csv";
  // let k_rev_start = 1.0;
  // let k_rev_step = 1.0;
  // let out_file = "dreamrunner_btc_1d_optimal_backtest.png";
  // let ticker = "BTCUSD".to_string();

  let strategy = Dreamrunner::new(
    "ATLASUSD".to_string(),
    0.00001 ,
    Source::Close ,
    Source::Open,
    10 ,
    Some(1.0)
  );
  let time_series = "atlasusd_1h.csv";
  let k_rev_start = 0.00001;
  let k_rev_step = 0.00001;
  let out_file = "dreamrunner_atlas_1h_optimal_backtest.png";
  let ticker = "ATLASUSD".to_string();

  let capital = 1_000.0;
  let fee = 0.02;
  let bet = Bet::Percent(100.0);
  let leverage = 1;
  let short_selling = false;

  let start_time = Time::new(2022, &Month::from_num(12), &Day::from_num(5), None, None, None);
  let end_time = Time::new(2023, &Month::from_num(10), &Day::from_num(25), None, None, None);

  let csv = PathBuf::from(time_series);
  let mut backtest = Backtest::new(strategy.clone(), capital, fee, bet, leverage, short_selling);
  let csv_series = Dataframe::csv_series(&csv, Some(start_time), Some(end_time), ticker.clone())?;

  #[derive(Debug, Clone)]
  struct BacktestResult {
    pub k_rev: f64,
    pub wma_period: usize,
    pub summary: Summary
  }

  let mut results: Vec<BacktestResult> = (0..10).collect::<Vec<usize>>().into_par_iter().flat_map(|i| {
    let k_rev = trunc!(k_rev_start + (i as f64 * k_rev_step), 7);

    let results: Vec<BacktestResult> = (1..11).collect::<Vec<usize>>().into_par_iter().flat_map(|j| {
      let wma_period = j + 1;
      let mut strat = strategy.clone();
      strat.ma_period = wma_period;
      strat.k_rev = k_rev;
      let mut backtest = Backtest::new(strat, capital, fee, bet, leverage, short_selling);
      backtest.candles.insert(ticker.clone(), csv_series.candles.clone());
      let summary = backtest.backtest()?;
      let res = BacktestResult {
        k_rev,
        wma_period,
        summary
      };
      Result::<_, anyhow::Error>::Ok(res)
    }).collect();
    Result::<_, anyhow::Error>::Ok(results)
  }).flatten().collect();

  results.retain(|r| r.summary.total_trades(&ticker) > 1);

  // sort for highest percent ROI first
  results.sort_by(|a, b| {
    b.summary.pct_roi(&ticker).partial_cmp(&a.summary.pct_roi(&ticker)).unwrap()
  });

  if !results.is_empty() {
    let optimized = results.first().unwrap().clone();
    println!("==== Optimized Backtest ====");
    println!("WMA Period: {}", optimized.wma_period);
    println!("Kagi Rev: {}", optimized.k_rev);
    let summary = optimized.summary;
    summary.print(&ticker);
    backtest.candles.insert(ticker.clone(), csv_series.candles);
    let all_buy_and_hold = backtest.buy_and_hold()?;
    let _buy_and_hold = all_buy_and_hold
      .get(&ticker)
      .ok_or(anyhow::anyhow!("Buy and hold not found for ticker"))?
      .clone();
    Plot::plot(
      vec![summary.cum_pct(&ticker)?.data().clone(), _buy_and_hold],
      out_file,
      "Dreamrunner Optimal Backtest",
      "% ROI",
      "Unix Millis"
    )?;
  }


  Ok(())
}