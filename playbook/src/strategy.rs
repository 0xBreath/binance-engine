use time_series::{Signal, RollingCandles, Candle};

pub trait Strategy: Clone {
  /// Receives new candle and returns a signal (long, short, or do nothing).
  fn process_candle(&mut self, candle: Candle) -> anyhow::Result<Signal>;
  /// Appends a candle to the candle cache
  fn push_candle(&mut self, candle: Candle);
  /// Returns a reference to the candle cache
  fn candles(&self) -> &RollingCandles;
}