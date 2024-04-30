use log::warn;
use rayon::prelude::*;
use crate::Strategy;
use time_series::{Candle, Signal, DataCache, Data, Dataset, SignalInfo, Time};
use tradestats::metrics::*;

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
  pub zscore_threshold: f64
}

impl StatArb {
  pub fn new(capacity: usize, window: usize, zscore_threshold: f64, x_ticker: String, y_ticker: String) -> Self {
    Self {
      capacity,
      window,
      x: DataCache::new(capacity, x_ticker),
      y: DataCache::new(capacity, y_ticker),
      zscore_threshold
    }
  }

  /// ZScore of last index in a spread time series
  pub fn zscore(series: &[f64], window: usize) -> anyhow::Result<f64> {
    // Guard: Ensure correct window size
    if window > series.len() {
      return Err(anyhow::anyhow!("Window size is greater than vector length"));
    }
    
    // last z score 
    let window_data: &[f64] = &series[series.len()-window..];
    let mean: f64 = window_data.iter().sum::<f64>() / window_data.len() as f64;
    let var: f64 = window_data.iter().map(|&val| (val - mean).powi(2)).sum::<f64>() / (window_data.len()-1) as f64;
    let std_dev: f64 = var.sqrt();
    if std_dev == 0.0 {
      return Err(anyhow::anyhow!("Standard deviation is zero"));
    }
    let z_score = (series[series.len()-1] - mean) / std_dev;
    Ok(z_score)
  }

  pub fn signal(&mut self) -> anyhow::Result<Vec<Signal>> {
    if self.x.vec.len() < self.x.capacity || self.y.vec.len() < self.y.capacity {
      warn!("Insufficient candles to generate signal");
      return Ok(vec![Signal::None, Signal::None]);
    }
    
    let x: Vec<f64> = self.x.vec.clone().into_par_iter().map(|d| d.y).collect();
    let x_0 = self.x.vec[0].clone();
    let x_1 = self.x.vec[1].clone();

    let y: Vec<f64> = self.y.vec.clone().into_par_iter().map(|d| d.y).collect();
    let y_0 = self.y.vec[0].clone();
    let y_1 = self.y.vec[1].clone();
    
    if x_0.x != y_0.x || x_1.x != y_1.x {
      warn!("Data cache timestamps are not aligned");
      return Ok(vec![Signal::None, Signal::None]);
    }
    
    let spread: Vec<f64> = spread_dynamic(&x, &y).map_err(
      |e| anyhow::anyhow!("Error calculating dynamic spread: {}", e)
    )?;
    assert_eq!(spread.len(), y.len());
    assert_eq!(spread.len(), x.len());
    let s_0 = Data {
      x: x_0.x,
      y: spread[spread.len() - 1]
    };
    let s_1 = Data {
      x: x_1.x,
      y: spread[spread.len() - 2]
    };
    
    let z_0 = Data {
      x: x_0.x,
      y: Self::zscore(&spread, self.window)?
    };
    let z_1 = Data {
      x: x_1.x,
      y: Self::zscore(&spread[..spread.len() - 1], self.window)?
    };
    
    // let zscore: Vec<f64> = rolling_zscore(&spread, self.window).map_err(
    //   |e| anyhow::anyhow!("Error calculating rolling zscore: {}", e)
    // )?;
    // assert_eq!(zscore.len(), spread.len());
    // let zscore = Dataset::new(zscore.iter().enumerate().map(|(i, x)| Data { x: i as i64, y: *x }).collect());
    // let zscores = zscore.asc_order();
    // let z_0 = zscores[0].clone();
    // let z_1 = zscores[1].clone();
      
    let enter_long = z_0.y < -self.zscore_threshold; // below -1st std dev
    let exit_long = z_0.y > 0.0 && z_1.y < 0.0; // returns to mean (zscore = 0)

    match (enter_long, exit_long) {
      (true, true) => {
        Err(anyhow::anyhow!("Both long and short signals detected"))
      },
      (true, false) => {
        // Ok(vec![
        //   Signal::Long(SignalInfo {
        //     price: y_0.y,
        //     date: Time::from_unix_ms(y_0.x),
        //     ticker: self.y.id.clone()
        //   }),
        //   Signal::Short(SignalInfo {
        //     price: x_0.y,
        //     date: Time::from_unix_ms(x_0.x),
        //     ticker: self.x.id.clone()
        //   })
        // ])
        Ok(vec![
            Signal::Short(SignalInfo {
              price: y_0.y,
              date: Time::from_unix_ms(y_0.x),
              ticker: self.y.id.clone()
            }),
            Signal::Long(SignalInfo {
              price: x_0.y,
              date: Time::from_unix_ms(x_0.x),
              ticker: self.x.id.clone() 
          })
        ])
      },
      (false, true) => {
        // Ok(vec![
        //   Signal::Short(SignalInfo {
        //     price: y_0.y,
        //     date: Time::from_unix_ms(y_0.x),
        //     ticker: self.y.id.clone()
        //   }),
        //   Signal::Long(SignalInfo {
        //     price: x_0.y,
        //     date: Time::from_unix_ms(x_0.x),
        //     ticker: self.x.id.clone()
        //   })
        // ])
        Ok(vec![
          Signal::Long(SignalInfo {
              price: y_0.y,
              date: Time::from_unix_ms(y_0.x),
              ticker: self.y.id.clone()
            }),
            Signal::Short(SignalInfo {
              price: x_0.y,
              date: Time::from_unix_ms(x_0.x),
              ticker: self.x.id.clone()
            })
        ])
      },
      (false, false) => Ok(vec![Signal::None, Signal::None])
    }
  }
}

impl Strategy<Data<f64>> for StatArb {
  /// Appends candle to candle cache and returns a signal (long, short, or do nothing).
  fn process_candle(&mut self, candle: Candle, ticker: Option<String>) -> anyhow::Result<Vec<Signal>> {
    self.push_candle(candle, ticker);
    self.signal()
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
}


// ==========================================================================================
//                                 StatArb Backtests
// ==========================================================================================

#[tokio::test]
async fn btc_eth_backtest() -> anyhow::Result<()> {
  use super::*;
  use std::path::PathBuf;
  use time_series::{Time, Day, Month, Plot};
  use crate::Backtest;
  use std::collections::HashSet;
  dotenv::dotenv().ok();

  let start_time = Time::new(2023, &Month::from_num(1), &Day::from_num(1), None, None, None);
  // let start_time = Time::new(2024, &Month::from_num(4), &Day::from_num(1), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(30), None, None, None);
  
  let capacity = 1_000;
  let window = 20;
  
  // let capacity = 10_000;
  // let window = 100;
  
  // let capacity = 1000;
  // let window = 100;
    
  let threshold = 1.0;
  let x_ticker = "BTCUSDT".to_string();
  let y_ticker = "ETHUSDT".to_string();
  let strat = StatArb::new(capacity, window,  threshold, x_ticker.clone(), y_ticker.clone());
  let stop_loss = 100.0;

  let mut backtest = Backtest::new(strat.clone(), 1000.0, 0.02, true, 1);
  let btc_csv = PathBuf::from("btcusdt_30m.csv");
  let mut btc_candles = backtest.csv_series(&btc_csv, Some(start_time), Some(end_time), x_ticker.clone())?.candles;
  let eth_csv = PathBuf::from("ethusdt_30m.csv");
  let mut eth_candles = backtest.csv_series(&eth_csv, Some(start_time), Some(end_time), y_ticker.clone())?.candles;

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
  backtest.backtest(stop_loss)?;

  let all_buy_and_hold = backtest.buy_and_hold()?;
  if let Some(trades) = backtest.trades.get(&x_ticker) {
    if !trades.is_empty() {
      let x_summary = backtest.summary(x_ticker.clone())?;
      let x_bah = all_buy_and_hold
        .get(&x_ticker)
        .ok_or(anyhow::anyhow!("Buy and hold not found for ticker"))?
        .clone();
      Plot::plot(
        vec![x_summary.cum_pct.data().clone(), x_bah],
        "btcusdt_30m_stat_arb_backtest.png",
        "BTCUSDT Stat Arb Backtest",
        "% ROI",
        "Unix Millis"
      )?;
    }
  }

  if let Some(trades) = backtest.trades.get(&y_ticker) {
    if !trades.is_empty() {
      let y_summary = backtest.summary(y_ticker.clone())?;
      let y_bah = all_buy_and_hold
        .get(&y_ticker)
        .ok_or(anyhow::anyhow!("Buy and hold not found for ticker"))?
        .clone();
      Plot::plot(
        vec![y_summary.cum_pct.data().clone(), y_bah],
        "ethusdt_30m_stat_arb_backtest.png",
        "ETHUSDT Stat Arb Backtest",
        "% ROI",
        "Unix Millis"
      )?;
    }
  }


  Ok(())
}

#[tokio::test]
async fn btc_eth_spread_zscore() -> anyhow::Result<()> {
  use super::*;
  use std::path::PathBuf;
  use time_series::{Time, Day, Month, Plot};
  use crate::Backtest;
  use tradestats::metrics::*;
  use tradestats::utils::*;
  use std::collections::HashSet;
  dotenv::dotenv().ok();

  let start_time = Time::new(2023, &Month::from_num(1), &Day::from_num(1), None, None, None);
  // let start_time = Time::new(2024, &Month::from_num(4), &Day::from_num(20), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(30), None, None, None);

  let capacity = 1000;
  let window = 20;
  let threshold = 1.0;
  let x_ticker = "BTCUSDT".to_string();
  let y_ticker = "ETHUSDT".to_string();
  let strat = StatArb::new(capacity, window, threshold, x_ticker.clone(), y_ticker.clone());

  let mut backtest = Backtest::new(strat.clone(), 1000.0, 0.0, false, 1);
  let btc_csv = PathBuf::from("btcusdt_30m.csv");
  let mut btc_candles = backtest.csv_series(&btc_csv, Some(start_time), Some(end_time), x_ticker.clone())?.candles;
  let eth_csv = PathBuf::from("ethusdt_30m.csv");
  let mut eth_candles = backtest.csv_series(&eth_csv, Some(start_time), Some(end_time), y_ticker.clone())?.candles;

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
  let x = log_returns(&x, true);
  let y = log_returns(&y, true);
  let x: Vec<f64> = x.into_iter().take(1000).collect();
  let y: Vec<f64> = y.into_iter().take(1000).collect();
  assert_eq!(x.len(), y.len());

  let correlation = rolling_correlation(&x, &y, window).map_err(
    |e| anyhow::anyhow!("Error calculating rolling correlation: {}", e)
  )?;
  assert_eq!(correlation.len(), y.len());
  assert_eq!(correlation.len(), x.len());
  let correlation = Dataset::new(correlation.iter().enumerate().map(|(i, x)| Data { x: i as i64, y: *x }).collect());
  Plot::plot(
    vec![correlation.data().clone()],
    "btc_eth_correlation.png",
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
    "btc_eth_spread_zscore.png",
    "BTC/ETH Spread Z Score",
    "Z Score",
    "Time"
  )?;

  Ok(())
}

#[tokio::test]
async fn btc_eth_cointegration() -> anyhow::Result<()> {
  use super::*;
  use std::path::PathBuf;
  use time_series::{Time, Day, Month, Plot};
  use crate::Backtest;
  use tradestats::kalman::*;
  use tradestats::metrics::*;
  use tradestats::utils::*;
  use std::collections::HashSet;
  dotenv::dotenv().ok();

  let start_time = Time::new(2023, &Month::from_num(1), &Day::from_num(1), None, None, None);
  // let start_time = Time::new(2024, &Month::from_num(4), &Day::from_num(20), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(30), None, None, None);

  let capacity = 1000;
  let window = 20;
  let threshold = 1.0;
  let x_ticker = "BTCUSDT".to_string();
  let y_ticker = "ETHUSDT".to_string();
  let strat = StatArb::new(capacity, window, threshold, x_ticker.clone(), y_ticker.clone());

  let mut backtest = Backtest::new(strat.clone(), 1000.0, 0.0, false, 1);
  let btc_csv = PathBuf::from("btcusdt_30m.csv");
  let mut btc_candles = backtest.csv_series(&btc_csv, Some(start_time), Some(end_time), x_ticker.clone())?.candles;
  let eth_csv = PathBuf::from("ethusdt_30m.csv");
  let mut eth_candles = backtest.csv_series(&eth_csv, Some(start_time), Some(end_time), y_ticker.clone())?.candles;

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
  let half_life: f64 = half_life(&spread).unwrap();
  println!("Half-life: {} bars", half_life);

  let half_life = half_life.abs().round() as usize;
  println!("Half-life: {} bars", half_life);

  let window = 20;
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
    vec![roll_coint.data().clone()],
    "btc_eth_rolling_coint.png",
    "BTC/ETH Rolling Cointegration",
    "Cointegration",
    "Time"
  )?;

  Ok(())
}