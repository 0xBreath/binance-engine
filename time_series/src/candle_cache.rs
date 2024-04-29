use std::collections::VecDeque;
use crate::Candle;

#[derive(Debug, Clone)]
pub struct CandleCache {
  pub vec: VecDeque<Candle>,
  pub capacity: usize,
  pub ticker: String
}
impl CandleCache {
  pub fn new(capacity: usize, ticker: String) -> Self {
    Self {
      vec: VecDeque::with_capacity(capacity),
      capacity,
      ticker
    }
  }

  pub fn push(&mut self, candle: Candle) {
    if self.vec.len() == self.capacity {
      self.vec.pop_back();
    }
    self.vec.push_front(candle);
  }
  
  pub fn recent(&self) -> Option<&Candle> {
    self.vec.front()
  }
}