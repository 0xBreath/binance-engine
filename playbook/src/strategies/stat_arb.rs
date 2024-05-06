#![allow(unused_imports)]

use log::{warn};
use rayon::prelude::*;
use crate::Strategy;
use time_series::*;
use tradestats::kalman::*;
use tradestats::metrics::*;
use tradestats::utils::*;
use std::path::PathBuf;
use crate::Backtest;
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct StatArb {
  /// Capacity of data caches
  pub capacity: usize,
  /// Window to compute zscores
  pub window: usize,
  /// Last N data from current datum.
  /// 0th index is current datum, Nth index is oldest datum.
  pub x: DataCache<Data<f64>>,
  /// Last N data from current datum.
  /// 0th index is current datum, Nth index is oldest datum.
  pub y: DataCache<Data<f64>>,
  pub zscore_threshold: f64,
  pub stop_loss_pct: Option<f64>
}

impl StatArb {
  pub fn new(capacity: usize, window: usize, zscore_threshold: f64, x_ticker: String, y_ticker: String, stop_loss_pct: Option<f64>) -> Self {
    Self {
      capacity,
      window,
      x: DataCache::new(capacity, x_ticker),
      y: DataCache::new(capacity, y_ticker),
      zscore_threshold,
      stop_loss_pct
    }
  }

  /// ZScore of last index in a spread time series
  pub fn zscore(series: &[f64], window: usize) -> anyhow::Result<f64> {
    // Guard: Ensure correct window size
    if window > series.len() {
      return Err(anyhow::anyhow!("Window size is greater than vector length"));
    }
    
    // last z score 
    let window_data: &[f64] = &series[series.len() - window..];
    let mean: f64 = window_data.iter().sum::<f64>() / window_data.len() as f64;
    let var: f64 = window_data.iter().map(|&val| (val - mean).powi(2)).sum::<f64>() / (window_data.len()-1) as f64;
    let std_dev: f64 = var.sqrt();
    if std_dev == 0.0 {
      return Err(anyhow::anyhow!("Standard deviation is zero"));
    }
    let z_score = (series[series.len()-1] - mean) / std_dev;
    Ok(z_score)
  }

  pub fn signal(&mut self, ticker: Option<String>) -> anyhow::Result<Vec<Signal>> {
    match ticker {
      None => Ok(vec![]),
      Some(ticker) => {
        if self.x.vec.len() < self.x.capacity || self.y.vec.len() < self.y.capacity {
          warn!("Insufficient candles to generate signal");
          return Ok(vec![]);
        }

        let x: Vec<f64> = self.x.vec.clone().into_par_iter().rev().map(|d| d.y).collect();
        let x_0 = self.x.vec[0].clone();

        let y: Vec<f64> = self.y.vec.clone().into_par_iter().rev().map(|d| d.y).collect();
        let y_0 = self.y.vec[0].clone();

        let spread: Vec<f64> = spread_dynamic(&x, &y).map_err(
          |e| anyhow::anyhow!("Error calculating dynamic spread: {}", e)
        )?;
        assert_eq!(spread.len(), y.len());
        assert_eq!(spread.len(), x.len());
        let lag_spread = spread[..spread.len()-1].to_vec();

        let z_0 = Data {
          x: x_0.x,
          y: Self::zscore(&spread, self.window)?
        };
        let z_1 = Data {
          x: x_0.x,
          y: Self::zscore(&lag_spread, self.window)?
        };

        let enter_long = z_0.y < -self.zscore_threshold;
        let exit_long = z_0.y > 0.0 && z_1.y < 0.0;
        let enter_short = z_0.y > self.zscore_threshold;
        let exit_short = z_0.y < 0.0 && z_1.y > 0.0;
        // let enter_short = exit_long;
        // let exit_short = enter_long;

        let x_info = SignalInfo {
          price: x_0.y,
          date: Time::from_unix_ms(x_0.x),
          ticker: self.x.id.clone()
        };
        let y_info = SignalInfo {
          price: y_0.y,
          date: Time::from_unix_ms(y_0.x),
          ticker: self.y.id.clone()
        };

        let mut signals = vec![];

        // y increases, spread increases, short y
        // y decreases, spread decreases, long y

        // process exits before any new entries
        if exit_long {
          if ticker == self.x.id {
            // signals.push(Signal::EnterLong(x_info.clone()))
            signals.push(Signal::ExitLong(x_info.clone()))
          } else if ticker == self.y.id {
            signals.push(Signal::ExitLong(y_info.clone()))
          }
        }
        if exit_short {
          if ticker == self.x.id {
            // signals.push(Signal::EnterShort(x_info.clone()))
            signals.push(Signal::ExitShort(x_info.clone()))
          } else if ticker == self.y.id {
            signals.push(Signal::ExitShort(y_info.clone()))
          }
        }

        if enter_long {
          if ticker == self.x.id {
            // signals.push(Signal::ExitLong(x_info.clone()))
            signals.push(Signal::EnterLong(x_info.clone()))
          } else if ticker == self.y.id {
            signals.push(Signal::EnterLong(y_info.clone()))
          }
        }
        if enter_short {
          if ticker == self.x.id {
            // signals.push(Signal::ExitShort(x_info))
            signals.push(Signal::EnterShort(x_info))
          } else if ticker == self.y.id {
            signals.push(Signal::EnterShort(y_info))
          }
        }
        Ok(signals)
      }
    }

  }
}

impl Strategy<Data<f64>> for StatArb {
  /// Appends candle to candle cache and returns a signal (long, short, or do nothing).
  fn process_candle(&mut self, candle: Candle, ticker: Option<String>) -> anyhow::Result<Vec<Signal>> {
    self.push_candle(candle, ticker.clone());
    self.signal(ticker)
  }

  fn push_candle(&mut self, candle: Candle, ticker: Option<String>) {
    if let Some(ticker) = ticker {
      if ticker == self.x.id {
        self.x.push(Data {
          x: candle.date.to_unix_ms(),
          y: candle.close
        });
      } else if ticker == self.y.id {
        self.y.push(Data {
          x: candle.date.to_unix_ms(),
          y: candle.close
        });
      }
    }
  }

  fn cache(&self, ticker: Option<String>) -> Option<&DataCache<Data<f64>>> {
    if let Some(ticker) = ticker{
      if ticker == self.x.id {
        Some(&self.x)
      } else if ticker == self.y.id {
        Some(&self.y)
      } else {
        None
      }
    } else {
      None
    }
  }
  
  fn stop_loss_pct(&self) -> Option<f64> {
    self.stop_loss_pct
  }
}


// ==========================================================================================
//                                 StatArb 30m Backtests
// ==========================================================================================

#[tokio::test]
async fn btc_eth_30m_stat_arb() -> anyhow::Result<()> {
  use super::*;
  use std::path::PathBuf;
  use time_series::{Time, Day, Month, Plot, hurst, trunc, Bet};
  use crate::Backtest;
  use std::collections::HashSet;
  dotenv::dotenv().ok();

  let start_time = Time::new(2023, &Month::from_num(1), &Day::from_num(1), None, None, None);
  // let start_time = Time::new(2024, &Month::from_num(4), &Day::from_num(10), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(30), None, None, None);

  let capacity = 100;
  let window = 10;
  let threshold = 2.0;
  let stop_loss = 100.0;
  let fee = 0.02;
  let bet = Bet::Percent(100.0);
  let leverage = 1;
  let short_selling = false;

  let x_ticker = "BTCUSDT".to_string();
  let y_ticker = "ETHUSDT".to_string();
  
  let btc_csv = PathBuf::from("btcusdt_30m.csv");
  let mut btc_candles = Backtest::<Data<f64>, StatArb>::csv_series(&btc_csv, Some(start_time), Some(end_time), x_ticker.clone())?.candles;
  let eth_csv = PathBuf::from("ethusdt_30m.csv");
  let mut eth_candles = Backtest::<Data<f64>, StatArb>::csv_series(&eth_csv, Some(start_time), Some(end_time), y_ticker.clone())?.candles;

  Backtest::<Data<f64>, StatArb>::align_pair_series(&mut btc_candles, &mut eth_candles)?;
  assert_eq!(btc_candles.len(), eth_candles.len());

  // let x: Vec<f64> = btc_candles.clone().into_par_iter().map(|d| d.close).collect();
  // let y: Vec<f64> = eth_candles.clone().into_par_iter().map(|d| d.close).collect();
  // normalize data using percent change from first price in time series
  let x_0 = btc_candles[0];
  let y_0 = eth_candles[0];
  let x: Dataset<f64> = Dataset::new(btc_candles.iter().map(|c| Data {
    x: c.date.to_unix_ms(),
    y: (c.close / x_0.close - 1.0) * 100.0
  }).collect());
  let y: Dataset<f64> = Dataset::new(eth_candles.iter().map(|c| Data {
    x: c.date.to_unix_ms(),
    y: (c.close / y_0.close - 1.0) * 100.0
  }).collect());
  
  let spread: Vec<f64> = spread_standard(&x.y(), &y.y()).map_err(
    |e| anyhow::anyhow!("Error calculating spread: {}", e)
  )?;
  println!("Spread Hurst Exponent: {}", trunc!(hurst(spread.clone()), 2));
  
  let half_life = half_life(&spread).unwrap();
  println!("Spread half life: {}", half_life);
  // let window = half_life.abs().round() as usize;
  // let capacity = window + 2;

  let strat = StatArb::new(capacity, window, threshold, x_ticker.clone(), y_ticker.clone(), Some(stop_loss));
  let mut backtest = Backtest::new(strat.clone(), 1000.0, fee, bet, leverage, short_selling);
  // Append to backtest data
  backtest.candles.insert(x_ticker.clone(), btc_candles.clone());
  backtest.candles.insert(y_ticker.clone(), eth_candles.clone());
  println!("Backtest BTC candles: {}", backtest.candles.get(&x_ticker).unwrap().len());
  println!("Backtest ETH candles: {}", backtest.candles.get(&y_ticker).unwrap().len());

  let summary = backtest.backtest()?;
  let all_buy_and_hold = backtest.buy_and_hold()?;

  if let Some(trades) = backtest.trades.get(&x_ticker) {
    if trades.len() > 1 {
      summary.print(&x_ticker);
      let x_bah = all_buy_and_hold
        .get(&x_ticker)
        .ok_or(anyhow::anyhow!("Buy and hold not found for ticker"))?
        .clone();
      Plot::plot(
        vec![summary.cum_pct(&x_ticker)?.data().clone(), x_bah],
        "stat_arb_btc_30m_backtest.png",
        "BTCUSDT Stat Arb Backtest",
        "% ROI",
        "Unix Millis"
      )?;

      Plot::plot(
        vec![summary.pct_per_trade(&x_ticker)?.data().clone()],
        "stat_arb_btc_30m_trades.png",
        "BTCUSDT Stat Arb Trades",
        "% ROI",
        "Unix Millis"
      )?;

    }
  }
  if let Some(trades) = backtest.trades.get(&y_ticker) {
    if trades.len() > 1 {
      summary.print(&y_ticker);
      let y_bah = all_buy_and_hold
        .get(&y_ticker)
        .ok_or(anyhow::anyhow!("Buy and hold not found for ticker"))?
        .clone();
      Plot::plot(
        vec![summary.cum_pct(&y_ticker)?.data().clone(), y_bah],
        "stat_arb_eth_30m_backtest.png",
        "ETHUSDT Stat Arb Backtest",
        "% ROI",
        "Unix Millis"
      )?;

      Plot::plot(
        vec![summary.pct_per_trade(&y_ticker)?.data().clone()],
        "stat_arb_eth_30m_trades.png",
        "ETHUSDT Stat Arb Trades",
        "% ROI",
        "Unix Millis"
      )?;
    }
  }
  
  Ok(())
}

#[tokio::test]
async fn btc_eth_30m_spread_zscore() -> anyhow::Result<()> {
  use super::*;
  dotenv::dotenv().ok();

  let start_time = Time::new(2023, &Month::from_num(1), &Day::from_num(1), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(30), None, None, None);

  let capacity = 10_000;
  let window = 20;
  let threshold = 1.0;
  let x_ticker = "BTCUSDT".to_string();
  let y_ticker = "ETHUSDT".to_string();
  let strat = StatArb::new(capacity, window, threshold, x_ticker.clone(), y_ticker.clone(), None);

  let mut backtest = Backtest::new(strat.clone(), 1000.0, 0.0, Bet::Static, 1, false);
  let btc_csv = PathBuf::from("btcusdt_30m.csv");
  let mut btc_candles = Backtest::<Data<f64>, StatArb>::csv_series(&btc_csv, Some(start_time), Some(end_time), x_ticker.clone())?.candles;
  let eth_csv = PathBuf::from("ethusdt_30m.csv");
  let mut eth_candles = Backtest::<Data<f64>, StatArb>::csv_series(&eth_csv, Some(start_time), Some(end_time), y_ticker.clone())?.candles;

  Backtest::<Data<f64>, StatArb>::align_pair_series(&mut btc_candles, &mut eth_candles)?;
  // Append to backtest data
  backtest.candles.insert(x_ticker.clone(), btc_candles);
  backtest.candles.insert(y_ticker.clone(), eth_candles);

  println!("Backtest BTC candles: {}", backtest.candles.get(&x_ticker).unwrap().len());
  println!("Backtest ETH candles: {}", backtest.candles.get(&y_ticker).unwrap().len());
  
  // normalize data using percent change from first price in time series
  let x_0 = backtest.candles.get(&x_ticker).unwrap()[0];
  let y_0 = backtest.candles.get(&y_ticker).unwrap()[0];
  let x: Dataset<f64> = Dataset::new(backtest.candles.get(&x_ticker).unwrap().iter().map(|c| Data {
    x: c.date.to_unix_ms(),
    y: (c.close / x_0.close - 1.0) * 100.0
  }).collect());
  let y: Dataset<f64> = Dataset::new(backtest.candles.get(&y_ticker).unwrap().iter().map(|c| Data {
    x: c.date.to_unix_ms(),
    y: (c.close / y_0.close - 1.0) * 100.0
  }).collect());
  assert_eq!(x.len(), y.len());

  Plot::plot(
    vec![x.data().clone(), y.data().clone()],
    "btc_eth_30m_normalized.png",
    "BTC & ETH Percent Changes",
    "Percent from Origin",
    "Unix Millis"
  )?;

  let correlation = rolling_correlation(&x.y(), &y.y(), window).map_err(
    |e| anyhow::anyhow!("Error calculating rolling correlation: {}", e)
  )?;
  assert_eq!(correlation.len(), y.len());
  assert_eq!(correlation.len(), x.len());
  let correlation = Dataset::new(correlation.iter().enumerate().map(|(i, x)| Data { x: i as i64, y: *x }).collect());
  Plot::plot(
    vec![correlation.data().clone()],
    "btc_eth_30m_correlation.png",
    "BTC/ETH Correlation",
    "Correlation",
    "Time"
  )?;

  let spread: Vec<f64> = spread_standard(&x.y(), &y.y()).map_err(
    |e| anyhow::anyhow!("Error calculating dynamic spread: {}", e)
  )?;
  let spread_data = Dataset::new(spread.iter().enumerate().map(|(i, y)| {
    let x = x.x()[i];
    Data { x, y: *y }
  }).collect());
  Plot::plot(
    vec![spread_data.data().clone()],
    "btc_eth_30m_spread.png",
    "BTC/ETH Spread",
    "Spread",
    "Unix Millis"
  )?;
  
  assert_eq!(spread.len(), y.len());
  assert_eq!(spread.len(), x.len());
  let zscore: Vec<f64> = rolling_zscore(&spread, window).unwrap();
  assert_eq!(zscore.len(), spread.len());
  let zscore = Dataset::new(zscore.iter().enumerate().map(|(i, x)| Data { x: i as i64, y: *x }).collect());
  Plot::plot(
    vec![zscore.data().clone()],
    "btc_eth_30m_spread_zscore.png",
    "BTC/ETH Spread Z Score",
    "Z Score",
    "Time"
  )?;
  
  let half_life: f64 = half_life(&spread).unwrap();
  let half_life = half_life.abs().round() as usize;
  println!("Spread half-life: {} bars", half_life);

  Ok(())
}

#[tokio::test]
async fn btc_eth_30m_cointegration() -> anyhow::Result<()> {
  use super::*;
  dotenv::dotenv().ok();

  let start_time = Time::new(2023, &Month::from_num(1), &Day::from_num(1), None, None, None);
  // let start_time = Time::new(2024, &Month::from_num(4), &Day::from_num(20), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(30), None, None, None);

  let capacity = 1000;
  let window = 20;
  let threshold = 1.0;
  let x_ticker = "BTCUSDT".to_string();
  let y_ticker = "ETHUSDT".to_string();
  let strat = StatArb::new(capacity, window, threshold, x_ticker.clone(), y_ticker.clone(), None);

  let mut backtest = Backtest::new(strat.clone(), 1000.0, 0.0, Bet::Static, 1, false);
  let btc_csv = PathBuf::from("btcusdt_30m.csv");
  let mut btc_candles = Backtest::<Data<f64>, StatArb>::csv_series(&btc_csv, Some(start_time), Some(end_time), x_ticker.clone())?.candles;
  let eth_csv = PathBuf::from("ethusdt_30m.csv");
  let mut eth_candles = Backtest::<Data<f64>, StatArb>::csv_series(&eth_csv, Some(start_time), Some(end_time), y_ticker.clone())?.candles;

  // retain the overlapping dates between the two time series
  // Step 1: Create sets of timestamps from both vectors
  let btc_dates: HashSet<i64> = btc_candles.iter().map(|c| c.date.to_unix_ms()).collect();
  let eth_dates: HashSet<i64> = eth_candles.iter().map(|c| c.date.to_unix_ms()).collect();
  // Step 2: Find the intersection of both timestamp sets
  let common_timestamps: HashSet<&i64> = btc_dates.intersection(&eth_dates).collect();
  // Step 3: Filter each vector to keep only the common timestamps
  btc_candles.retain(|c| common_timestamps.contains(&c.date.to_unix_ms()));
  eth_candles.retain(|c| common_timestamps.contains(&c.date.to_unix_ms()));

  // Step 4: Sort both vectors by timestamp to ensure they are aligned
  // earliest point in time is 0th index, latest point in time is Nth index
  btc_candles.sort_by_key(|c| c.date.to_unix_ms());
  eth_candles.sort_by_key(|c| c.date.to_unix_ms());
  // Append to backtest data
  backtest.candles.insert(x_ticker.clone(), btc_candles);
  backtest.candles.insert(y_ticker.clone(), eth_candles);

  println!("Backtest BTC candles: {}", backtest.candles.get(&x_ticker).unwrap().len());
  println!("Backtest ETH candles: {}", backtest.candles.get(&y_ticker).unwrap().len());

  // let x: Vec<f64> = backtest.candles.get(&x_ticker).unwrap().iter().map(|c| c.close).collect();
  // let y: Vec<f64> = backtest.candles.get(&y_ticker).unwrap().iter().map(|c| c.close).collect();
  // normalize data using percent change from first price in time series
  let x_0 = backtest.candles.get(&x_ticker).unwrap()[0];
  let y_0 = backtest.candles.get(&y_ticker).unwrap()[0];
  let x: Dataset<f64> = Dataset::new(backtest.candles.get(&x_ticker).unwrap().iter().map(|c| Data {
    x: c.date.to_unix_ms(),
    y: (c.close / x_0.close - 1.0) * 100.0
  }).collect());
  let y: Dataset<f64> = Dataset::new(backtest.candles.get(&y_ticker).unwrap().iter().map(|c| Data {
    x: c.date.to_unix_ms(),
    y: (c.close / y_0.close - 1.0) * 100.0
  }).collect());
  assert_eq!(x.len(), y.len());

  assert_eq!(x.len(), y.len());

  let dynamic_kalman_hedge = Dataset::new(dynamic_hedge_kalman_filter(&x.y(), &y.y()).map_err(
    |e| anyhow::anyhow!("Error calculating dynamic hedge ratio: {}", e)
  )?.iter().enumerate().map(|(i, x)| Data { x: i as i64, y: *x }).collect());

  let coint = engle_granger_cointegration_test(&x.y(), &y.y()).map_err(
    |e| anyhow::anyhow!("Error calculating Engle-Granger cointegration test: {}", e)
  )?;
  println!("Engle-Granger Cointegration Test: {:#?}", coint);

  let spread: Vec<f64> = spread_standard(&x.y(), &y.y()).map_err(
    |e| anyhow::anyhow!("Error calculating dynamic spread: {}", e)
  )?;
  assert_eq!(spread.len(), y.len());
  assert_eq!(spread.len(), x.len());


  let half_life: f64 = half_life(&spread).unwrap();
  // let window = 20;
  let window = half_life.abs().round() as usize;

  let roll_coint = Dataset::new(rolling_cointegration(&x.y(), &y.y(), window).map_err(
    |e| anyhow::anyhow!("Error calculating rolling cointegration: {}", e)
  )?.iter().enumerate().map(|(i, x)| Data { x: i as i64, y: *x }).collect());

  Plot::plot(
    vec![dynamic_kalman_hedge.data().clone()],
    "btc_eth_30m_dynamic_kalman_hedge.png",
    "BTC/ETH Dynamic Kalman Filter Hedge",
    "Hedge Ratio",
    "Time"
  )?;
  Plot::plot(
    vec![roll_coint.data().clone()],
    "btc_eth_30m_rolling_coint.png",
    "BTC/ETH Rolling Cointegration",
    "Cointegration",
    "Time"
  )?;

  Ok(())
}



// ==========================================================================================
//                                 StatArb 1D Backtests
// ==========================================================================================



#[tokio::test]
async fn btc_eth_1d_stat_arb() -> anyhow::Result<()> {
  use super::*;
  dotenv::dotenv().ok();

  let start_time = Time::new(2014, &Month::from_num(1), &Day::from_num(1), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(5), &Day::from_num(1), None, None, None);

  let capacity = 100;
  let window = 10;
  let threshold = 2.0;
  let stop_loss = 100.0;
  let fee = 0.02;
  let bet = Bet::Percent(100.0);
  let leverage = 1;
  let short_selling = false;

  let x_ticker = "BTCUSD".to_string();
  let y_ticker = "ETHUSD".to_string();
  let btc_csv = PathBuf::from("btcusd_1d.csv");
  let mut btc_candles = Backtest::<Data<f64>, StatArb>::csv_series(&btc_csv, Some(start_time), Some(end_time), x_ticker.clone())?.candles;
  let eth_csv = PathBuf::from("ethusd_1d.csv");
  let mut eth_candles = Backtest::<Data<f64>, StatArb>::csv_series(&eth_csv, Some(start_time), Some(end_time), y_ticker.clone())?.candles;

  // retain the overlapping dates between the two time series
  // Step 1: Create sets of timestamps from both vectors
  let btc_dates: HashSet<i64> = btc_candles.iter().map(|c| c.date.to_unix_ms()).collect();
  let eth_dates: HashSet<i64> = eth_candles.iter().map(|c| c.date.to_unix_ms()).collect();
  // Step 2: Find the intersection of both timestamp sets
  let common_timestamps: HashSet<&i64> = btc_dates.intersection(&eth_dates).collect();
  // Step 3: Filter each vector to keep only the common timestamps
  btc_candles.retain(|c| common_timestamps.contains(&c.date.to_unix_ms()));
  eth_candles.retain(|c| common_timestamps.contains(&c.date.to_unix_ms()));

  // Step 4: Sort both vectors by timestamp to ensure they are aligned
  // earliest point in time is 0th index, latest point in time is Nth index
  btc_candles.sort_by_key(|c| c.date.to_unix_ms());
  eth_candles.sort_by_key(|c| c.date.to_unix_ms());

  let x: Vec<f64> = btc_candles.clone().into_par_iter().map(|d| d.close).collect();
  let y: Vec<f64> = eth_candles.clone().into_par_iter().map(|d| d.close).collect();
  let spread: Vec<f64> = spread_standard(&x, &y).map_err(
    |e| anyhow::anyhow!("Error calculating dynamic spread: {}", e)
  )?;
  println!("Spread Hurst Exponent: {}", trunc!(hurst(spread.clone()), 2));
  let half_life = half_life(&spread).unwrap();
  println!("Spread half life: {}", half_life);
  // let window = half_life.abs().round() as usize;
  // let capacity = window + 2;
  
  let strat = StatArb::new(capacity, window, threshold, x_ticker.clone(), y_ticker.clone(), Some(stop_loss));
  let mut backtest = Backtest::new(strat.clone(), 1000.0, fee, bet, leverage, short_selling);
  // Append to backtest data
  backtest.candles.insert(x_ticker.clone(), btc_candles.clone());
  backtest.candles.insert(y_ticker.clone(), eth_candles.clone());
  println!("Backtest BTC candles: {}", backtest.candles.get(&x_ticker).unwrap().len());
  println!("Backtest ETH candles: {}", backtest.candles.get(&y_ticker).unwrap().len());

  let summary = backtest.backtest()?;
  let all_buy_and_hold = backtest.buy_and_hold()?;

  if let Some(trades) = backtest.trades.get(&x_ticker) {
    if trades.len() > 1 {
      summary.print(&x_ticker);
      let _x_bah = all_buy_and_hold
        .get(&x_ticker)
        .ok_or(anyhow::anyhow!("Buy and hold not found for ticker"))?
        .clone();
      Plot::plot(
        vec![summary.cum_pct(&x_ticker)?.data().clone(), _x_bah],
        // vec![summary.cum_pct(&x_ticker)?.data().clone()],
        "stat_arb_btc_1d_backtest.png",
        "BTCUSD Stat Arb Backtest",
        "% ROI",
        "Unix Millis"
      )?;

      Plot::plot(
        vec![summary.pct_per_trade(&x_ticker)?.data().clone()],
        "stat_arb_btc_1d_trades.png",
        "BTCUSD Stat Arb Trades",
        "% ROI",
        "Unix Millis"
      )?;

    }
  }
  if let Some(trades) = backtest.trades.get(&y_ticker) {
    if trades.len() > 1 {
      summary.print(&y_ticker);
      let _y_bah = all_buy_and_hold
        .get(&y_ticker)
        .ok_or(anyhow::anyhow!("Buy and hold not found for ticker"))?
        .clone();
      Plot::plot(
        vec![summary.cum_pct(&y_ticker)?.data().clone(), _y_bah],
        // vec![summary.cum_pct(&y_ticker)?.data().clone()],
        "stat_arb_eth_1d_backtest.png",
        "ETHUSD Stat Arb Backtest",
        "% ROI",
        "Unix Millis"
      )?;

      Plot::plot(
        vec![summary.pct_per_trade(&y_ticker)?.data().clone()],
        "stat_arb_eth_1d_trades.png",
        "ETHUSD Stat Arb Trades",
        "% ROI",
        "Unix Millis"
      )?;
    }
  }

  Ok(())
}

#[tokio::test]
async fn btc_eth_1d_spread_zscore() -> anyhow::Result<()> {
  use super::*;
  dotenv::dotenv().ok();

  let start_time = Time::new(2013, &Month::from_num(1), &Day::from_num(1), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(5), &Day::from_num(1), None, None, None);

  let capacity = 10_000;
  let window = 20;
  let threshold = 1.0;
  let x_ticker = "BTCUSD".to_string();
  let y_ticker = "ETHUSD".to_string();
  let strat = StatArb::new(capacity, window, threshold, x_ticker.clone(), y_ticker.clone(), None);

  let mut backtest = Backtest::new(strat.clone(), 1000.0, 0.0, Bet::Static, 1, false);
  let btc_csv = PathBuf::from("btcusd_1d.csv");
  let mut btc_candles = Backtest::<Data<f64>, StatArb>::csv_series(&btc_csv, Some(start_time), Some(end_time), x_ticker.clone())?.candles;
  let eth_csv = PathBuf::from("ethusd_1d.csv");
  let mut eth_candles = Backtest::<Data<f64>, StatArb>::csv_series(&eth_csv, Some(start_time), Some(end_time), y_ticker.clone())?.candles;

  // retain the overlapping dates between the two time series
  // Step 1: Create sets of timestamps from both vectors
  let btc_dates: HashSet<i64> = btc_candles.iter().map(|c| c.date.to_unix_ms()).collect();
  let eth_dates: HashSet<i64> = eth_candles.iter().map(|c| c.date.to_unix_ms()).collect();
  // Step 2: Find the intersection of both timestamp sets
  let common_timestamps: HashSet<&i64> = btc_dates.intersection(&eth_dates).collect();
  // Step 3: Filter each vector to keep only the common timestamps
  btc_candles.retain(|c| common_timestamps.contains(&c.date.to_unix_ms()));
  eth_candles.retain(|c| common_timestamps.contains(&c.date.to_unix_ms()));

  // Step 4: Sort both vectors by timestamp to ensure they are aligned
  // earliest point in time is 0th index, latest point in time is Nth index
  btc_candles.sort_by_key(|c| c.date.to_unix_ms());
  eth_candles.sort_by_key(|c| c.date.to_unix_ms());
  // Append to backtest data
  backtest.candles.insert(x_ticker.clone(), btc_candles);
  backtest.candles.insert(y_ticker.clone(), eth_candles);

  println!("Backtest BTC candles: {}", backtest.candles.get(&x_ticker).unwrap().len());
  println!("Backtest ETH candles: {}", backtest.candles.get(&y_ticker).unwrap().len());

  let x: Vec<f64> = backtest.candles.get(&x_ticker).unwrap().iter().map(|c| c.close).collect();
  let y: Vec<f64> = backtest.candles.get(&y_ticker).unwrap().iter().map(|c| c.close).collect();
  assert_eq!(x.len(), y.len());

  let correlation = rolling_correlation(&x, &y, window).map_err(
    |e| anyhow::anyhow!("Error calculating rolling correlation: {}", e)
  )?;
  assert_eq!(correlation.len(), y.len());
  assert_eq!(correlation.len(), x.len());
  let correlation = Dataset::new(correlation.iter().enumerate().map(|(i, x)| Data { x: i as i64, y: *x }).collect());
  Plot::plot(
    vec![correlation.data().clone()],
    "btc_eth_1d_correlation.png",
    "BTC/ETH Correlation",
    "Correlation",
    "Time"
  )?;

  let spread: Vec<f64> = spread_dynamic(&x, &y).map_err(
    |e| anyhow::anyhow!("Error calculating dynamic spread: {}", e)
  )?;
  assert_eq!(spread.len(), y.len());
  assert_eq!(spread.len(), x.len());
  let zscore: Vec<f64> = rolling_zscore(&spread, window).unwrap();
  assert_eq!(zscore.len(), spread.len());
  let zscore = Dataset::new(zscore.iter().enumerate().map(|(i, x)| Data { x: i as i64, y: *x }).collect());
  Plot::plot(
    vec![zscore.data().clone()],
    "btc_eth_30m_spread_zscore.png",
    "BTC/ETH Spread Z Score",
    "Z Score",
    "Time"
  )?;

  // let spread: Vec<f64> = spread.into_iter().take(10000).collect();
  let half_life: f64 = half_life(&spread).unwrap();
  let half_life = half_life.abs().round() as usize;
  println!("Spread half-life: {} bars", half_life);

  Ok(())
}

#[tokio::test]
async fn btc_eth_1d_cointegration() -> anyhow::Result<()> {
  use super::*;
  dotenv::dotenv().ok();

  let start_time = Time::new(2014, &Month::from_num(1), &Day::from_num(1), None, None, None);
  // let start_time = Time::new(2024, &Month::from_num(4), &Day::from_num(20), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(5), &Day::from_num(1), None, None, None);

  let capacity = 1000;
  let window = 20;
  let threshold = 1.0;
  let x_ticker = "BTCUSD".to_string();
  let y_ticker = "ETHUSD".to_string();
  let strat = StatArb::new(capacity, window, threshold, x_ticker.clone(), y_ticker.clone(), None);

  let mut backtest = Backtest::new(strat.clone(), 1000.0, 0.0, Bet::Static, 1, false);
  let btc_csv = PathBuf::from("btcusd_1d.csv");
  let mut btc_candles = Backtest::<Data<f64>, StatArb>::csv_series(&btc_csv, Some(start_time), Some(end_time), x_ticker.clone())?.candles;
  let eth_csv = PathBuf::from("ethusd_1d.csv");
  let mut eth_candles = Backtest::<Data<f64>, StatArb>::csv_series(&eth_csv, Some(start_time), Some(end_time), y_ticker.clone())?.candles;

  // retain the overlapping dates between the two time series
  // Step 1: Create sets of timestamps from both vectors
  let btc_dates: HashSet<i64> = btc_candles.iter().map(|c| c.date.to_unix_ms()).collect();
  let eth_dates: HashSet<i64> = eth_candles.iter().map(|c| c.date.to_unix_ms()).collect();
  // Step 2: Find the intersection of both timestamp sets
  let common_timestamps: HashSet<&i64> = btc_dates.intersection(&eth_dates).collect();
  // Step 3: Filter each vector to keep only the common timestamps
  btc_candles.retain(|c| common_timestamps.contains(&c.date.to_unix_ms()));
  eth_candles.retain(|c| common_timestamps.contains(&c.date.to_unix_ms()));

  // Step 4: Sort both vectors by timestamp to ensure they are aligned
  // earliest point in time is 0th index, latest point in time is Nth index
  btc_candles.sort_by_key(|c| c.date.to_unix_ms());
  eth_candles.sort_by_key(|c| c.date.to_unix_ms());
  // Append to backtest data
  backtest.candles.insert(x_ticker.clone(), btc_candles);
  backtest.candles.insert(y_ticker.clone(), eth_candles);

  println!("Backtest BTC candles: {}", backtest.candles.get(&x_ticker).unwrap().len());
  println!("Backtest ETH candles: {}", backtest.candles.get(&y_ticker).unwrap().len());

  let x: Vec<f64> = backtest.candles.get(&x_ticker).unwrap().iter().map(|c| c.close).collect();
  let y: Vec<f64> = backtest.candles.get(&y_ticker).unwrap().iter().map(|c| c.close).collect();
  let x = log_returns(&x, false);
  let y = log_returns(&y, false);
  assert_eq!(x.len(), y.len());

  let dynamic_kalman_hedge = Dataset::new(dynamic_hedge_kalman_filter(&x, &y).map_err(
    |e| anyhow::anyhow!("Error calculating dynamic hedge ratio: {}", e)
  )?.iter().enumerate().map(|(i, x)| Data { x: i as i64, y: *x }).collect());

  let coint = engle_granger_cointegration_test(&x, &y).map_err(
    |e| anyhow::anyhow!("Error calculating Engle-Granger cointegration test: {}", e)
  )?;
  println!("Engle-Granger Cointegration Test: {:#?}", coint);

  let spread: Vec<f64> = spread_dynamic(&x, &y).map_err(
    |e| anyhow::anyhow!("Error calculating dynamic spread: {}", e)
  )?;
  assert_eq!(spread.len(), y.len());
  assert_eq!(spread.len(), x.len());

  let window = 20;
  let roll_coint = rolling_cointegration(&x, &y, window).map_err(
    |e| anyhow::anyhow!("Error calculating rolling cointegration: {}", e)
  )?;

  let half_life: f64 = half_life(&roll_coint).unwrap();
  let half_life = half_life.abs().round() as usize;
  println!("Rolling cointegration half-life: {} bars", half_life);

  let roll_coint = Dataset::new(rolling_cointegration(&x, &y, window).map_err(
    |e| anyhow::anyhow!("Error calculating rolling cointegration: {}", e)
  )?.iter().enumerate().map(|(i, x)| Data { x: i as i64, y: *x }).collect());

  Plot::plot(
    vec![dynamic_kalman_hedge.data().clone()],
    "btc_eth_1d_dynamic_kalman_hedge.png",
    "BTC/ETH Dynamic Kalman Filter Hedge",
    "Hedge Ratio",
    "Time"
  )?;
  Plot::plot(
    vec![roll_coint.data().clone()],
    "btc_eth_1d_rolling_coint.png",
    "BTC/ETH Rolling Cointegration",
    "Cointegration",
    "Time"
  )?;

  Ok(())
}