use std::collections::VecDeque;
use crate::Candle;

#[derive(Debug, Clone)]
pub struct RollingCandles {
  pub vec: VecDeque<Candle>,
  pub capacity: usize,
}
impl RollingCandles {
  pub fn new(capacity: usize) -> Self {
    Self {
      vec: VecDeque::with_capacity(capacity),
      capacity,
    }
  }

  pub fn push(&mut self, candle: Candle) {
    if self.vec.len() == self.capacity {
      self.vec.pop_back();
    }
    self.vec.push_front(candle);
  }
}