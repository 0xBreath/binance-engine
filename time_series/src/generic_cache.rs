// use std::collections::VecDeque;
// use crate::{Candle, Data};
// use serde::{Serialize, Deserialize};
// 
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct GenericData<T> {
//   pub x: i64,
//   pub y: T,
// }
// 
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct GenericDataset<T>(pub Vec<GenericData<T>>);
// 
// impl<T> GenericDataset<T> {
//   pub fn new(data: Vec<GenericData<T>>) -> Self { Self(data) }
// 
//   pub fn asc_order(&self) -> Vec<GenericData<T>> {
//     // sort so data.x is in ascending order (highest value is 0th index)
//     let mut data = self.0.clone();
//     data.sort_by(|a, b| a.x.cmp(&b.x));
//     data
//   }
// 
//   pub fn x(&self) -> Vec<i64> {
//     self.0.iter().map(|d| d.x).collect()
//   }
// 
//   pub fn y(&self) -> Vec<f64> {
//     self.0.iter().map(|d| d.y).collect()
//   }
// 
//   pub fn data(&self) -> &Vec<Data> {
//     &self.0
//   }
// 
//   pub fn len(&self) -> usize {
//     self.0.len()
//   }
// 
//   pub fn is_empty(&self) -> bool {
//     self.0.is_empty()
//   }
// 
//   pub fn min_x(&self) -> f64 {
//     self.0.iter().map(|d| d.y).fold(f64::INFINITY, |a, b| a.min(b))
//   }
// 
//   pub fn max_x(&self) -> f64 {
//     self.0.iter().map(|d| d.y).fold(f64::NEG_INFINITY, |a, b| a.max(b))
//   }
// 
//   pub fn min_y(&self) -> f64 {
//     self.0.iter().map(|d| d.y).fold(f64::INFINITY, |a, b| a.min(b))
//   }
// 
//   pub fn max_y(&self) -> f64 {
//     self.0.iter().map(|d| d.y).fold(f64::NEG_INFINITY, |a, b| a.max(b))
//   }
// }
// 
// #[derive(Debug, Clone)]
// pub struct DataCache {
//   pub vec: VecDeque<GenericData<T>>,
//   pub capacity: usize,
//   pub ticker: String
// }
// impl CandleCache {
//   pub fn new(capacity: usize, ticker: String) -> Self {
//     Self {
//       vec: VecDeque::with_capacity(capacity),
//       capacity,
//       ticker
//     }
//   }
// 
//   pub fn push(&mut self, candle: Candle) {
//     if self.vec.len() == self.capacity {
//       self.vec.pop_back();
//     }
//     self.vec.push_front(candle);
//   }
// 
//   pub fn recent(&self) -> Option<&Data<T>> {
//     self.vec.front()
//   }
// 
//   // convert VecDeque to slice
//   pub fn vec(&self) -> Vec<Data<T>> {
//     self.vec.iter().cloned().collect::<Vec<Data<T>>>()
//   }
// }