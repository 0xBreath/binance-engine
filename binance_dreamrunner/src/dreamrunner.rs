use std::collections::VecDeque;
use log::{info, warn};
use time_series::{Candle, trunc};
use crate::kagi::Kagi;
use crate::utils::{Source, Signal};

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
  pub fn signal(&self, candles: VecDeque<Candle>) -> anyhow::Result<Signal> {
    if candles.len() < 3 {
      warn!("Insufficient candles to generate Kagis");
      return Ok(Signal::None);
    }
    if candles.len() < candles.capacity() {
      warn!("Insufficient candles to generate WMA");
      return Ok(Signal::None);
    }
    info!("len: {}, cap: {}", candles.len(), candles.capacity());
    let c_0 = candles[0];
    let c_1 = candles[1];
    let c_2 = candles[2];
    let k_0 = Kagi::new(self.k_rev, &c_0, &c_1);
    info!("kagi: {:#?}", k_0);
    let k_1 = Kagi::new(self.k_rev, &c_1, &c_2);
    let period_from_curr: Vec<&Candle> = candles.range(0..candles.len() - 1).collect();
    let period_from_prev: Vec<&Candle> = candles.range(1..candles.len()).collect();
    let wma_0 = self.wma(&period_from_curr);
    info!("wma: {}", wma_0);
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
    trunc!(sum / norm, 2)
  }
}