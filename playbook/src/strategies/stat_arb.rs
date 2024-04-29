use log::warn;
use crate::{Strategy};
use time_series::{Candle, Signal, Source, CandleCache, Data, Dataset, Op};

#[derive(Debug, Clone)]
pub struct StatArb {
  pub src: Source,
  pub period: usize,
  /// Last N candles from current candle.
  /// 0th index is current candle, Nth index is oldest candle.
  pub candles: CandleCache,
  pub threshold: f64
}

impl StatArb {
  pub fn new(src: Source, period: usize, threshold: f64) -> Self {
    Self {
      src,
      period,
      candles: CandleCache::new(period + 1),
      threshold
    }
  }
  
  pub fn test(&self) -> anyhow::Result<()> {
    Ok(())
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

impl Strategy for StatArb {
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
//                                 StatArb Backtests
// ==========================================================================================

#[tokio::test]
async fn btc_eth_kalman() -> anyhow::Result<()> {
  use super::*;
  use std::path::PathBuf;
  use time_series::{Time, Day, Month, Plot};
  use crate::Backtest;
  use tradestats::kalman::*;
  use tradestats::metrics::*;
  use tradestats::utils::*;
  dotenv::dotenv().ok();

  let start_time = Time::new(2023, &Month::from_num(1), &Day::from_num(1), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(24), None, None, None);
  
  let strat = StatArb::new(Source::Close, 50, 2.0);
  
  let mut btc = Backtest::new(strat.clone(), 0.0, 0.0, false, 1);
  let btc_csv = PathBuf::from("btcusdt_30m.csv");
  let btc_candles = btc.add_csv_series(&btc_csv, Some(start_time), Some(end_time))?.candles;

  let mut eth = Backtest::new(strat, 0.0, 0.0, false, 1);
  let eth_csv = PathBuf::from("ethusdt_30m.csv");
  let eth_candles = eth.add_csv_series(&eth_csv, Some(start_time), Some(end_time))?.candles;
  
  // clean btc and eth candles so they both have the same candle dates and length
  for (btc_candle, eth_candle) in btc_candles.iter().zip(eth_candles.iter()) {
    if btc_candle.date != eth_candle.date {
      continue;
    }
    btc.add_candle(*btc_candle);
    eth.add_candle(*eth_candle);
  }
  
  let x: Vec<f64> = btc.candles.iter().map(|c| c.close).collect();
  let y: Vec<f64> = eth.candles.iter().map(|c| c.close).collect();
  let x = log_returns(&x, false);
  let y = log_returns(&y, false);

  let spread: Vec<f64> = spread_dynamic(&x, &y).map_err(
    |e| anyhow::anyhow!("Error calculating dynamic spread: {}", e)
  )?;

  let dynamic_kalman_hedge = Dataset::new(dynamic_hedge_kalman_filter(&x, &y).map_err(
    |e| anyhow::anyhow!("Error calculating dynamic hedge ratio: {}", e)
  )?.iter().enumerate().map(|(i, x)| Data { x: i as i64, y: *x }).collect());
  
  let coint = engle_granger_cointegration_test(&x, &y).map_err(
    |e| anyhow::anyhow!("Error calculating Engle-Granger cointegration test: {}", e)
  )?;
  println!("Engle-Granger Cointegration Test: {:#?}", coint);
  
  let half_life: f64 = half_life(&spread).unwrap();
  println!("Half-life: {} bars", half_life);

  // let window = 20;
  let window = half_life.abs().round() as usize;
  let zscore: Vec<f64> = rolling_zscore(&spread, window).unwrap();
  let zscore = Dataset::new(zscore.iter().enumerate().map(|(i, x)| Data { x: i as i64, y: *x }).collect());
  
  let roll_coint = Dataset::new(rolling_cointegration(&x, &y, window).map_err(
    |e| anyhow::anyhow!("Error calculating rolling cointegration: {}", e)
  )?.iter().enumerate().map(|(i, x)| Data { x: i as i64, y: *x }).collect());

  Plot::plot(
    vec![dynamic_kalman_hedge.data().clone()],
    "btc_eth_dynamic_kalman_hedge.png",
    "BTC/ETH Dynamic Kalman Filter Hedge",
    "Hedge Ratio",
    "Time"
  )?;
  Plot::plot(
    vec![zscore.data().clone()],
    "btc_eth_spread_zscore.png",
    "BTC/ETH Spread Z Score",
    "Spread",
    "Time"
  )?;
  Plot::plot(
    vec![roll_coint.data().clone()],
    "btc_eth_rolling_coint.png",
    "BTC/ETH Rolling Cointegration",
    "Cointegration",
    "Time"
  )?;

  Ok(())
}