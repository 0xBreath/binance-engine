use log::{info, warn};
use time_series::{Candle, trunc};
use crate::kagi::Kagi;
use lib::trade::*;

#[derive(Debug, Clone, Copy)]
pub struct Dreamrunner {
  pub k_rev: f64,
  pub k_src: Source,
  pub ma_src: Source
}

impl Default for Dreamrunner {
  fn default() -> Self {
    Self {
      k_rev: 0.001,
      k_src: Source::Close,
      ma_src: Source::Open
    }
  }
}

impl Dreamrunner {
  pub fn signal(&self, kagi: &mut Kagi, candles: &RollingCandles) -> anyhow::Result<Signal> {
    if candles.vec.len() < 3 {
      warn!("Insufficient candles to generate kagis");
      return Ok(Signal::None);
    }
    if candles.vec.len() < candles.capacity {
      warn!("Insufficient candles to generate WMA");
      return Ok(Signal::None);
    }
    let c_0 = candles.vec[0];
    // old kagi
    let k_1 = *kagi;
    // new kagi
    let k_0 = Kagi::update(kagi, self.k_rev, &c_0);
    // update kagi
    kagi.line = k_0.line;
    kagi.direction = k_0.direction;
    
    info!("{:#?}", k_0);
    let period_from_curr: Vec<&Candle> = candles.vec.range(0..candles.vec.len() - 1).collect();
    let period_from_prev: Vec<&Candle> = candles.vec.range(1..candles.vec.len()).collect();
    let wma_0 = self.wma(&period_from_curr);
    info!("WMA: {}", trunc!(wma_0, 3));
    let wma_1 = self.wma(&period_from_prev);

    let x = wma_0 > k_0.line;
    let y = wma_0 < k_0.line;
    let x_1 = wma_1 > k_1.line;
    let y_1 = wma_1 < k_1.line;
    
    let long = x && !x_1;
    let short = y && !y_1;
    match (long, short) {
      (true, true) => {
        Err(anyhow::anyhow!("Both long and short signals detected"))
      },
      (true, false) => Ok(Signal::Long((c_0.close, c_0.date))),
      (false, true) => Ok(Signal::Short((c_0.close, c_0.date))),
      (false, false) => Ok(Signal::None)
    }
  }

  /// pinescript WMA source code:
  /// ```pinescript
  /// x = close // Source
  /// y = 5 // MA Period
  /// norm = 0.0
  /// sum = 0.0
  /// for i = 0 to y - 1
  ///    weight = (y - i) * y
  ///    norm := norm + weight
  ///    sum := sum + x[i] * weight
  /// sum / norm
  /// ```
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