use log::warn;
use crate::{Strategy};
use time_series::{Candle, Signal, CandleCache, Data, Dataset, SignalInfo};
use tradestats::metrics::*;

#[derive(Debug, Clone)]
pub struct StatArb {
  pub window: usize,
  /// Last N candles from current candle.
  /// 0th index is current candle, Nth index is oldest candle.
  pub x: CandleCache,
  /// Last N candles from current candle.
  /// 0th index is current candle, Nth index is oldest candle.
  pub y: CandleCache,
  pub zscore_threshold: f64
}

impl StatArb {
  pub fn new(window: usize, zscore_threshold: f64, x_ticker: String, y_ticker: String) -> Self {
    Self {
      window,
      x: CandleCache::new(window, x_ticker),
      y: CandleCache::new(window, y_ticker),
      zscore_threshold
    }
  }

  pub fn signal(&mut self) -> anyhow::Result<Vec<Signal>> {
    if self.x.vec.len() < self.x.capacity || self.y.vec.len() < self.y.capacity {
      warn!("Insufficient candles to generate signal");
      return Ok(vec![Signal::None, Signal::None]);
    }
    
    let x_d = Dataset::new(self.x.vec.iter().map(|c| {
      Data {
        x: c.date.to_unix_ms(),
        y: c.close
      }
    }).collect());
    let x = x_d.y();
    let x_0 = self.x.vec[0];

    let y_d = Dataset::new(self.y.vec.iter().map(|c| {
      Data {
        x: c.date.to_unix_ms(),
        y: c.close
      }
    }).collect());
    let y = y_d.y();
    let y_0 = self.y.vec[0];

    let spread: Vec<f64> = spread_dynamic(&x, &y).map_err(
      |e| anyhow::anyhow!("Error calculating dynamic spread: {}", e)
    )?;
    assert_eq!(spread.len(), y.len());
    assert_eq!(spread.len(), x.len());
    
    let zscore: Vec<f64> = rolling_zscore(&spread, self.window).unwrap();
    assert_eq!(zscore.len(), spread.len());
    let zscore = Dataset::new(zscore.iter().enumerate().map(|(i, x)| Data { x: i as i64, y: *x }).collect());
    let zscores = zscore.asc_order();
    let z_0 = zscores[0].clone();
    let z_1 = zscores[1].clone();
    
    let enter_long = z_0.y < -self.zscore_threshold; // above 1st std dev
    let exit_long = z_0.y > 0.0 && z_1.y < 0.0; // returns to mean (zscore = 0)
    
    match (enter_long, exit_long) {
      (true, true) => {
        Err(anyhow::anyhow!("Both long and short signals detected"))
      },
      (true, false) => Ok(vec![
        Signal::Short(SignalInfo {
          price: y_0.close, 
          date: y_0.date,
          ticker: self.y.ticker.clone()
        }), 
        Signal::Long(SignalInfo {
          price: x_0.close, 
          date: x_0.date,
          ticker: self.x.ticker.clone()
        })
      ]),
      (false, true) => Ok(vec![
        Signal::Long(SignalInfo {
          price: y_0.close,
          date: y_0.date,
          ticker: self.y.ticker.clone()
        }),
        Signal::Short(SignalInfo {
          price: x_0.close,
          date: x_0.date,
          ticker: self.x.ticker.clone()
        })
      ]),
      (false, false) => Ok(vec![Signal::None, Signal::None])
    }
  }
}

impl Strategy for StatArb {
  /// Appends candle to candle cache and returns a signal (long, short, or do nothing).
  fn process_candle(&mut self, candle: Candle, ticker: Option<String>) -> anyhow::Result<Vec<Signal>> {
    self.push_candle(candle, ticker);
    self.signal()
  }

  fn push_candle(&mut self, candle: Candle, ticker: Option<String>) {
    if let Some(ticker) = ticker {
      if ticker == self.x.ticker {
        self.x.push(candle);
      } else if ticker == self.y.ticker {
        self.y.push(candle);
      }
    }
  }

  fn candles(&self, ticker: Option<String>) -> Option<&CandleCache> {
    if let Some(ticker) = ticker{ 
      if ticker == self.x.ticker {
        Some(&self.x)
      } else if ticker == self.y.ticker {
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
async fn btc_eth_cointegration() -> anyhow::Result<()> {
  use super::*;
  use std::path::PathBuf;
  use time_series::{Time, Day, Month, Plot};
  use crate::Backtest;
  use tradestats::kalman::*;
  use tradestats::metrics::*;
  use tradestats::utils::*;
  dotenv::dotenv().ok();

  // let start_time = Time::new(2023, &Month::from_num(1), &Day::from_num(1), None, None, None);
  let start_time = Time::new(2024, &Month::from_num(4), &Day::from_num(20), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(24), None, None, None);
  
  let strat = StatArb::new(100, 1.0, "BTCUSDT".to_string(), "ETHUSDT".to_string());;
  
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
  assert_eq!(x.len(), y.len());

  let spread: Vec<f64> = spread_dynamic(&x, &y).map_err(
    |e| anyhow::anyhow!("Error calculating dynamic spread: {}", e)
  )?;
  assert_eq!(spread.len(), y.len());
  assert_eq!(spread.len(), x.len());

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
  let window = 20;
  let zscore: Vec<f64> = rolling_zscore(&spread, window).unwrap();
  assert_eq!(zscore.len(), spread.len());
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