#![allow(unused_imports)]

use log::warn;
use rayon::prelude::*;
use crate::Strategy;
use time_series::*;
use std::path::PathBuf;
use crate::Backtest;
use tradestats::metrics::*;

#[derive(Debug, Clone)]
pub struct HalfLife {
  /// Capacity of data caches
  pub capacity: usize,
  /// Window to compute zscores
  pub window: usize,
  /// Last N data from current datum.
  /// 0th index is current datum, Nth index is oldest datum.
  pub cache: DataCache<Data<i64, f64>>,
  pub zscore_threshold: f64,
  pub bars_since_entry: Option<usize>,
  pub stop_loss_pct: Option<f64>
}

impl HalfLife {
  pub fn new(capacity: usize, window: usize, zscore_threshold: f64, ticker: String, stop_loss_pct: Option<f64>) -> Self {
    Self {
      capacity,
      window,
      cache: DataCache::new(capacity, ticker),
      zscore_threshold,
      bars_since_entry: None,
      stop_loss_pct,
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

  pub fn signal(&mut self, ticker: Option<String>) -> anyhow::Result<Vec<Signal>> {
    match ticker {
      None => Ok(vec![]),
      Some(ticker) => {
        if self.cache.vec.len() < self.cache.capacity {
          warn!("Insufficient candles to generate signal");
          return Ok(vec![]);
        }
        if ticker != self.cache.id {
          return Ok(vec![]);
        }

        // most recent value is 0th index, so this is reversed to get oldest to newest
        let series: Vec<f64> = self.cache.vec.clone().into_par_iter().map(|d| d.y.ln()).collect();
        let spread: Vec<f64> = series.windows(2).map(|x| x[1] - x[0]).collect();
        let lag_spread = spread[..spread.len()-1].to_vec();

        let y_0 = self.cache.vec[0].clone();
        let y_1 = self.cache.vec[1].clone();

        let z_0 = Data {
          x: y_0.x,
          y: Self::zscore(&spread, self.window)?
        };
        let z_1 = Data {
          x: y_1.x,
          y: Self::zscore(&lag_spread, self.window)?
        };

        let enter_long = z_0.y < -self.zscore_threshold;
        let exit_long = z_0.y > 0.0 && z_1.y < 0.0;
        // let enter_short = z_0.y > self.zscore_threshold;
        // let exit_short = z_0.y < 0.0 && z_1.y > 0.0;
        let enter_short = exit_long;
        let exit_short = enter_long;

        let info = SignalInfo {
          price: y_0.y,
          date: Time::from_unix_ms(y_0.x()),
          ticker: ticker.clone()
        };
        let mut signals = vec![];
        // process exits before any new entries
        if exit_short {
          signals.push(Signal::ExitShort(info.clone()));
        }
        if exit_long {
          signals.push(Signal::ExitLong(info.clone()));
        }
        if enter_short {
          signals.push(Signal::EnterShort(info.clone()));
        }
        if enter_long {
          signals.push(Signal::EnterLong(info));
        }
        Ok(signals)
      }
    }
  }
}

impl Strategy<Data<i64, f64>> for HalfLife {
  /// Appends candle to candle cache and returns a signal (long, short, or do nothing).
  fn process_candle(&mut self, candle: Candle, ticker: Option<String>) -> anyhow::Result<Vec<Signal>> {
    self.push_candle(candle, ticker.clone());
    self.signal(ticker)
  }

  fn push_candle(&mut self, candle: Candle, ticker: Option<String>) {
    if let Some(ticker) = ticker {
      if ticker == self.cache.id {
        self.cache.push(Data {
          x: candle.x(),
          y: candle.y()
        });
      }
    }
  }

  fn cache(&self, ticker: Option<String>) -> Option<&DataCache<Data<i64, f64>>> {
    if let Some(ticker) = ticker{
      if ticker == self.cache.id {
        Some(&self.cache)
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
//                                 HalfLife Backtests
// ==========================================================================================

#[tokio::test]
async fn btc_30m_half_life() -> anyhow::Result<()> {
  use super::*;
  dotenv::dotenv().ok();

  let start_time = Time::new(2023, &Month::from_num(1), &Day::from_num(1), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(30), None, None, None);

  // BTCUSDT optimized
  // let capacity = 1_000;
  // let threshold = 2.0;
  // let ticker = "BTCUSDT".to_string();
  // let stop_loss = 0.1;
  // let fee = 0.02;
  // let compound = true;
  // let leverage = 1;
  // let short_selling = true;

  let capacity = 1_000;
  let threshold = 2.0;
  let ticker = "BTCUSDT".to_string();
  let stop_loss = 0.1;
  let fee = 0.02;
  let bet = Bet::Percent(100.0);
  let leverage = 1;
  let short_selling = true;

  let btc_csv = PathBuf::from("btcusdt_30m.csv");
  let mut btc_candles = Dataframe::csv_series(
    &btc_csv,
    Some(start_time),
    Some(end_time),
    ticker.clone()
  )?.candles;
  btc_candles.sort_by_key(|c| c.date.to_unix_ms());

  // convert to natural log to linearize time series
  let series: Vec<f64> = btc_candles.clone().into_iter().map(|d| d.close.ln()).collect();
  // let spread: Vec<f64> = series.windows(2).map(|x| x[1] - x[0]).collect();

  // half life of in-sample data (index 0 to 1000)
  let in_sample: Vec<f64> = series.clone().into_iter().take(1000).collect();
  let half_life = half_life(&in_sample).unwrap();
  println!("{} half-life: {} bars", ticker, trunc!(half_life, 1));
  // use this half-life as the strategy window
  // let window = half_life.abs().round() as usize;
  let window = 10;

  let strat = HalfLife::new(capacity, window, threshold, ticker.clone(), Some(stop_loss));
  let mut backtest = Backtest::new(
    strat.clone(),
    1_000.0,
    fee,
    bet,
    leverage,
    short_selling
  );

  // out-of-sample data (index 1000 to end)
  let btc_candles = btc_candles[1000..].to_vec();
  backtest.candles.insert(ticker.clone(), btc_candles);
  println!("Backtest BTC candles: {}", backtest.candles.get(&ticker).unwrap().len());

  let summary = backtest.backtest()?;
  summary.print(&ticker);

  if let Some(trades) = backtest.trades.get(&ticker) {
    if trades.len() > 1 {
      let bah = backtest.buy_and_hold()?
        .get(&ticker)
        .ok_or(anyhow::anyhow!("Buy and hold not found for ticker"))?
        .clone();
      Plot::plot(
        vec![summary.cum_pct(&ticker)?.data().clone(), bah],
        "half_life_btc_30m_backtest.png",
        "BTCUSDT Half Life Backtest",
        "% ROI",
        "Unix Millis"
      )?;
    }
  }

  Ok(())
}

#[tokio::test]
async fn btc_30m_hurst() -> anyhow::Result<()> {
  use super::*;
  dotenv::dotenv().ok();

  let start_time = Time::new(2023, &Month::from_num(1), &Day::from_num(1), None, None, None);
  // let start_time = Time::new(2024, &Month::from_num(4), &Day::from_num(1), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(30), None, None, None);

  let ticker = "BTCUSDT".to_string();

  let btc_csv = PathBuf::from("btcusdt_30m.csv");
  let mut btc_candles = Dataframe::csv_series(
    &btc_csv,
    Some(start_time),
    Some(end_time),
    ticker.clone()
  )?.candles;
  btc_candles.sort_by_key(|c| c.date.to_unix_ms());

  // convert to natural log to linearize time series
  let series: Vec<f64> = btc_candles.clone().into_iter().map(|d| d.close.ln()).collect();
  let spread: Vec<f64> = series.windows(2).map(|x| x[1] - x[0]).collect();
  
  let hurst_corr = hurst(spread);
  println!("{} hurst: {}", ticker, trunc!(hurst_corr, 2));

  Ok(())
}

#[tokio::test]
async fn btc_1d_half_life() -> anyhow::Result<()> {
  use super::*;
  dotenv::dotenv().ok();

  let start_time = Time::new(2012, &Month::from_num(1), &Day::from_num(1), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(5), &Day::from_num(1), None, None, None);
  
  let threshold = 2.0;
  let ticker = "BTCUSD".to_string();
  let stop_loss = 100.0;
  let fee = 0.02;
  let bet = Bet::Percent(100.0);
  let leverage = 1;
  let short_selling = false;

  let btc_csv = PathBuf::from("btcusd_1d.csv");
  let out_file = "half_life_btc_1d_backtest.png";
  let mut btc_candles = Dataframe::csv_series(
    &btc_csv,
    Some(start_time),
    Some(end_time),
    ticker.clone()
  )?.candles;
  btc_candles.sort_by_key(|c| c.date.to_unix_ms());
  
  let series: Vec<f64> = btc_candles.clone().into_iter().map(|d| d.close).collect();
  let sample_split = 1000;

  // half life of in-sample data (index 0 to 1000)
  let in_sample: Vec<f64> = series.clone().into_iter().take(sample_split).collect();
  let half_life = half_life(&in_sample).unwrap();
  println!("{} half-life: {} bars", ticker, trunc!(half_life, 1));
  // use this half-life as the strategy window
  let window = half_life.abs().round() as usize;

  let strat = HalfLife::new(window + 2, window, threshold, ticker.clone(), Some(stop_loss));
  let mut backtest = Backtest::new(
    strat.clone(),
    1_000.0,
    fee,
    bet,
    leverage,
    short_selling
  );

  // out-of-sample data (index sample split to end)
  let out_of_sample = btc_candles[sample_split..].to_vec();
  backtest.candles.insert(ticker.clone(), out_of_sample.clone());
  println!("Backtest BTC candles: {}", backtest.candles.get(&ticker).unwrap().len());

  let summary = backtest.backtest()?;
  summary.print(&ticker);

  if let Some(trades) = backtest.trades.get(&ticker) {
    if trades.len() > 1 {
      let _bah = backtest.buy_and_hold()?
        .get(&ticker)
        .ok_or(anyhow::anyhow!("Buy and hold not found for ticker"))?
        .clone();
      Plot::plot(
        // vec![summary.cum_pct(&ticker)?.data().clone(), _bah],
        vec![summary.cum_pct(&ticker)?.data().clone()],
        out_file,
        "BTCUSDT Half Life Backtest",
        "% ROI",
        "Unix Millis"
      )?;
    }
  }
  
  let zscores: Vec<f64> = rolling_zscore(&series, window).unwrap();
  let data: Vec<Data<i64, f64>> = zscores.into_iter().enumerate().map(|(i, z)| Data {
    x: btc_candles[i].x(),
    y: z
  }).collect();
  Plot::plot(
    vec![data],
    "half_life_btc_1d_zscores.png",
    "BTCUSD Half Life Z Scores",
    "Z Score",
    "Index"
  )?;

  Ok(())
}

/// BTC has a hurst of 0.6 on its lagged spread, which means it is trending/momentum.
/// BTCUSD normal price hurst: 1
/// BTCUSD normal spread hurst: 0.58
/// BTCUSD ln price hurst: 1.03
/// BTCUSD ln spread hurst: 0.6
#[tokio::test]
async fn btc_1d_hurst() -> anyhow::Result<()> {
  use super::*;
  dotenv::dotenv().ok();

  let start_time = Time::new(2012, &Month::from_num(1), &Day::from_num(1), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(5), &Day::from_num(1), None, None, None);

  let ticker = "BTCUSD".to_string();

  let btc_csv = PathBuf::from("btcusd_1d.csv");
  let mut btc_candles = Dataframe::csv_series(
    &btc_csv,
    Some(start_time),
    Some(end_time),
    ticker.clone()
  )?.candles;
  btc_candles.sort_by_key(|c| c.date.to_unix_ms());

  // convert to natural log to linearize time series
  let normal_series: Vec<f64> = btc_candles.clone().into_iter().map(|d| d.close).collect();
  let normal_spread: Vec<f64> = normal_series.windows(2).map(|x| x[1] - x[0]).collect();
  let ln_series: Vec<f64> = btc_candles.clone().into_iter().map(|d| d.close.ln()).collect();
  let ln_spread: Vec<f64> = ln_series.windows(2).map(|x| x[1] - x[0]).collect();

  println!("{} normal price hurst: {}", ticker, trunc!(hurst(normal_series), 2));
  println!("{} normal spread hurst: {}", ticker, trunc!(hurst(normal_spread), 2));
  println!("{} ln price hurst: {}", ticker, trunc!(hurst(ln_series), 2));
  println!("{} ln spread hurst: {}", ticker, trunc!(hurst(ln_spread), 2));

  Ok(())
}