use serde::{Serialize, Deserialize};

pub trait Y: Clone {
  fn y(&self) -> f64;
}

pub trait X: Clone {
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
pub struct Data<XX: X, YY: Y> {
  pub x: XX,
  pub y: YY,
}

impl<XX: X, YY: Y> Y for Data<XX, YY> {
  fn y(&self) -> f64 {
    self.y.y()
  }
}

impl<XX: X, YY: Y> X for Data<XX, YY> {
  fn x(&self) -> i64 {
    self.x.x()
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dataset<XX: X, YY: Y>(pub Vec<Data<XX, YY>>);

impl<T: X + Y> From<&[T]> for Dataset<i64, f64> {
  fn from(series: &[T]) -> Self {
    let data = series.iter().map(|d| Data {
      x: d.x(),
      y: d.y(),
    }).collect();
    Self(data)
  }
}

impl<XX: X, YY: Y> Dataset<XX, YY> {
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