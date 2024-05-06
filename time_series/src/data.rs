use serde::{Serialize, Deserialize};

pub trait Y {
  fn y(&self) -> f64;
}

pub trait X {
  fn x(&self) -> i64;
}

impl Y for f64 {
  fn y(&self) -> f64 {
    *self
  }
}

impl X for i64 {
  fn x(&self) -> i64 {
    *self
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Data<XX: Clone + X, YY: Clone + Y> {
  pub x: XX,
  pub y: YY,
}

impl<XX: Clone + X, YY: Clone + Y> Y for Data<XX, YY> {
  fn y(&self) -> f64 {
    self.y.y()
  }
}

impl<XX: Clone + X, YY: Clone + Y> X for Data<XX, YY> {
  fn x(&self) -> i64 {
    self.x.x()
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dataset<XX: Clone + X, YY: Clone + Y>(pub Vec<Data<XX, YY>>);

impl<XX: Clone + X, YY: Clone + Y> Dataset<XX, YY> {
  pub fn new(data: Vec<Data<XX, YY>>) -> Self { Self(data) }
  
  pub fn asc_order(&self) -> Vec<Data<XX, YY>> {
    // sort so data.x is in ascending order (highest value is 0th index)
    let mut data = self.0.clone();
    data.sort_by_key(|a| a.x());
    data
  }
  
  pub fn x(&self) -> Vec<i64> {
    self.0.iter().map(|d| d.x()).collect()
  }
  
  pub fn y(&self) -> Vec<f64> {
    self.0.iter().map(|d| d.y()).collect()
  }
  
  pub fn data(&self) -> &Vec<Data<XX, YY>> {
    &self.0
  }

  pub fn len(&self) -> usize {
    self.0.len()
  }

  pub fn is_empty(&self) -> bool {
    self.0.is_empty()
  }
}