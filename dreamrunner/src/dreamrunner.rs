use log::{info, warn};
use time_series::{Candle, Signal, Source, trunc, RollingCandles, Kagi};

#[derive(Debug, Clone, Copy)]
pub struct Dreamrunner {
  pub k_rev: f64,
  pub k_src: Source,
  pub ma_src: Source,
}

impl Default for Dreamrunner {
  fn default() -> Self {
    Self {
      k_rev: 0.03,
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
    
    // previous candle
    let c_1 = candles.vec[1];
    // current candle
    let c_0 = candles.vec[0];
    
    // kagi for previous candle
    let k_1 = *kagi;
    // kagi for current candle
    let k_0 = Kagi::update(kagi, self.k_rev, &c_0, &c_1);
    kagi.line = k_0.line;
    kagi.direction = k_0.direction;
    info!("{:#?}", k_0);
    
    let period_1: Vec<&Candle> = candles.vec.range(1..candles.vec.len()).collect();
    let period_0: Vec<&Candle> = candles.vec.range(0..candles.vec.len() - 1).collect();
    
    let wma_1 = self.wma(&period_1);
    let wma_0 = self.wma(&period_0);
    info!("WMA: {}", trunc!(wma_0, 3));
    
    // long if WMA crosses above Kagi and was below Kagi in previous candle
    let long = wma_0 > k_0.line && wma_1 < k_1.line;
    // short if WMA crosses below Kagi and was above Kagi in previous candle
    let short = wma_0 < k_0.line && wma_1 > k_1.line;

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
    sum / norm
  }
}