use crate::Time;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Event for a single candlestick for a given ticker.
#[derive(Clone, Debug)]
pub struct Candle {
    /// UNIX timestamp in seconds
    pub date: Time,
    /// Open price
    pub open: f64,
    /// High price
    pub high: f64,
    /// Low price
    pub low: f64,
    /// Close price
    pub close: f64,
    /// Volume
    pub volume: Option<f64>,
}

impl Candle {
    pub fn percent_change(&self, prev_close: f64) -> f64 {
        ((100.0 / prev_close) * self.close) - 100.0
    }
}

impl PartialEq for Candle {
    fn eq(&self, other: &Self) -> bool {
        self.date.to_string() == other.date.to_string() && self.close == other.close
    }
}

pub trait CandleTrait {
    fn unix_date(&self) -> u64;
}

impl CandleTrait for Candle {
    fn unix_date(&self) -> u64 {
        self.date.to_unix() as u64
    }
}

#[derive(Clone, Debug, Default)]
pub struct CandleHasher(pub DefaultHasher);

pub trait CandleHashTrait {
    fn new() -> Self;
    fn finish(&mut self) -> u64;
    fn hash_candle<T: CandleTrait>(&mut self, candle: &T) -> u64;
}

impl CandleHashTrait for CandleHasher {
    fn new() -> Self {
        Self(DefaultHasher::new())
    }
    /// Reset contents of hasher for reuse
    fn finish(&mut self) -> u64 {
        self.0.finish()
    }
    /// Hash account using key and slot
    fn hash_candle<T: CandleTrait>(&mut self, candle: &T) -> u64 {
        self.0 = DefaultHasher::new();
        candle.unix_date().hash(&mut self.0);
        self.finish()
    }
}
