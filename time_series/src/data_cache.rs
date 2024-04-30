use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct DataCache<T> {
  pub vec: VecDeque<T>,
  pub capacity: usize,
  pub id: String
}
impl<T: Clone> DataCache<T> {
  pub fn new(capacity: usize, id: String) -> Self {
    Self {
      vec: VecDeque::with_capacity(capacity),
      capacity,
      id
    }
  }

  pub fn push(&mut self, t: T) {
    if self.vec.len() == self.capacity {
      self.vec.pop_back();
    }
    self.vec.push_front(t);
  }

  pub fn recent(&self) -> Option<&T> {
    self.vec.front()
  }

  // convert VecDeque to slice
  pub fn vec(&self) -> Vec<T> {
    self.vec.iter().cloned().collect::<Vec<T>>()
  }
}