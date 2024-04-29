use time_series::{Signal, CandleCache, Candle};

pub trait Strategy: Clone {
  /// Receives new candle and returns a signal (long, short, or do nothing).
  fn process_candle(&mut self, candle: Candle, ticker: Option<String>) -> anyhow::Result<Vec<Signal>>;
  /// Appends a candle to the candle cache
  fn push_candle(&mut self, candle: Candle, ticker: Option<String>);
  /// Returns a reference to the candle cache
  fn candles(&self, ticker: Option<String>) -> Option<&CandleCache>;
}