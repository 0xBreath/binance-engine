use log::warn;
use rayon::prelude::*;
use crate::Strategy;
use time_series::{Candle, Signal, DataCache, Data, SignalInfo, Time};

#[derive(Debug, Clone)]
pub struct HalfLife {
  /// Capacity of data caches
  pub capacity: usize,
  /// Window to compute zscores
  pub window: usize,
  /// Last N data from current datum.
  /// 0th index is current datum, Nth index is oldest datum.
  pub cache: DataCache<Data<f64>>,
  pub zscore_threshold: f64,
  pub bars_since_entry: Option<usize>
}

impl HalfLife {
  pub fn new(capacity: usize, window: usize, zscore_threshold: f64, ticker: String) -> Self {
    Self {
      capacity,
      window,
      cache: DataCache::new(capacity, ticker),
      zscore_threshold,
      bars_since_entry: None
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

  pub fn signal(&mut self, ticker: Option<String>) -> anyhow::Result<Signal> {
    match ticker {
      None => Ok(Signal::None),
      Some(ticker) => {
        if self.cache.vec.len() < self.cache.capacity {
          warn!("Insufficient candles to generate signal");
          return Ok(Signal::None);
        }
        if ticker != self.cache.id {
          return Ok(Signal::None);
        }

        // most recent value is 0th index, so this is revered to get oldest to newest
        let series: Vec<f64> = self.cache.vec.clone().into_par_iter().rev().map(|d| d.y.ln()).collect();
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
        
        let enter_short = exit_long; // z_0.y > self.zscore_threshold;
        let exit_short = enter_long; // z_0.y < 0.0 && z_1.y > 0.0;

        let info = SignalInfo {
          price: y_0.y,
          date: Time::from_unix_ms(y_0.x),
          ticker: ticker.clone()
        };
        if enter_long {
          Ok(Signal::EnterLong(info))
        } else if exit_long {
          Ok(Signal::ExitLong(info))
        } else if enter_short {
          Ok(Signal::EnterShort(info))
        } else if exit_short {
          Ok(Signal::ExitShort(info))
        } else {
          Ok(Signal::None)
        }
      }
    }
  }
}

impl Strategy<Data<f64>> for HalfLife {
  /// Appends candle to candle cache and returns a signal (long, short, or do nothing).
  fn process_candle(&mut self, candle: Candle, ticker: Option<String>) -> anyhow::Result<Signal> {
    self.push_candle(candle, ticker.clone());
    self.signal(ticker)
  }

  fn push_candle(&mut self, candle: Candle, ticker: Option<String>) {
    if let Some(ticker) = ticker {
      if ticker == self.cache.id {
        self.cache.push(Data {
          x: candle.date.to_unix_ms(),
          y: candle.close
        });
      }
    }
  }

  fn cache(&self, ticker: Option<String>) -> Option<&DataCache<Data<f64>>> {
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
}


// ==========================================================================================
//                                 HalfLife Backtests
// ==========================================================================================

#[tokio::test]
async fn btc_half_life() -> anyhow::Result<()> {
  use super::*;
  use std::path::PathBuf;
  use time_series::{Time, Day, Month, Plot, trunc};
  use crate::Backtest;
  use tradestats::metrics::*;
  dotenv::dotenv().ok();

  let start_time = Time::new(2023, &Month::from_num(1), &Day::from_num(1), None, None, None);
  // let start_time = Time::new(2024, &Month::from_num(4), &Day::from_num(1), None, None, None);
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
  let compound = true;
  let leverage = 1;
  let short_selling = true;
  
  let btc_csv = PathBuf::from("btcusdt_30m.csv");
  let mut btc_candles = Backtest::<Data<f64>, HalfLife>::csv_series(
    &btc_csv,
    Some(start_time),
    Some(end_time),
    ticker.clone()
  )?.candles;
  btc_candles.sort_by_key(|c| c.date.to_unix_ms());

  // convert to natural log to linearize time series
  let series: Vec<f64> = btc_candles.clone().into_iter().map(|d| d.close.ln()).collect();

  // half life of in-sample data (index 0 to 1000)
  let in_sample: Vec<f64> = series.clone().into_iter().take(1000).collect();
  let half_life = half_life(&in_sample).unwrap();
  println!("{} half-life: {} bars", ticker, trunc!(half_life, 1));
  // use this half-life as the strategy window
  // let window = half_life.abs().round() as usize;
  let window = 10;

  let strat = HalfLife::new(capacity, window, threshold, ticker.clone());
  let mut backtest = Backtest::new(
    strat.clone(),
    1_000.0,
    fee,
    compound,
    leverage,
    short_selling
  );
  // backtest.candles.insert(ticker.clone(), btc_candles.clone());
  // println!("Backtest BTC candles: {}", backtest.candles.get(&ticker).unwrap().len());

  // out-of-sample data (index 1000 to end)
  let btc_candles = btc_candles[1000..].to_vec();
  backtest.candles.insert(ticker.clone(), btc_candles);
  println!("Backtest BTC candles: {}", backtest.candles.get(&ticker).unwrap().len());

  backtest.backtest(stop_loss)?;
  if let Some(trades) = backtest.trades.get(&ticker) {
    if trades.len() > 1 {
      let summary = backtest.summary(ticker.clone())?;
      summary.print();
      let bah = backtest.buy_and_hold()?
        .get(&ticker)
        .ok_or(anyhow::anyhow!("Buy and hold not found for ticker"))?
        .clone();
      Plot::plot(
        vec![summary.cum_pct.data().clone(), bah],
        "half_life_btc_backtest.png",
        "BTCUSDT Half Life Backtest",
        "% ROI",
        "Unix Millis"
      )?;
    }
  }

  Ok(())
}