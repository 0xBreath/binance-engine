use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Data<T: Clone> {
  pub x: i64,
  pub y: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dataset<T: Clone>(pub Vec<Data<T>>);

impl<T: Clone> Dataset<T> {
  pub fn new(data: Vec<Data<T>>) -> Self { Self(data) }
  
  pub fn asc_order(&self) -> Vec<Data<T>> {
    // sort so data.x is in ascending order (highest value is 0th index)
    let mut data = self.0.clone();
    data.sort_by(|a, b| a.x.cmp(&b.x));
    data
  }
  
  pub fn x(&self) -> Vec<i64> {
    self.0.iter().map(|d| d.x).collect()
  }
  
  pub fn y(&self) -> Vec<T> {
    self.0.iter().map(|d| d.y.clone()).collect()
  }
  
  pub fn data(&self) -> &Vec<Data<T>> {
    &self.0
  }

  pub fn len(&self) -> usize {
    self.0.len()
  }

  pub fn is_empty(&self) -> bool {
    self.0.is_empty()
  }
}