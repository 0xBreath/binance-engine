use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum Op {
  None,
  Ln,
  Log10,
  ZScoreMean(usize),
  ZScoreMedian(usize),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Data {
  pub x: i64,
  pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dataset(pub Vec<Data>);

impl Dataset {
  pub fn new(data: Vec<Data>) -> Self {
    Self(data)
  }
  
  pub fn data(&self) -> &Vec<Data> {
    &self.0
  }
  
  pub fn translate(&self, op: &Op) -> Vec<Data> {
    match op {
      Op::None => self.0.clone(),
      Op::Ln => self.0.iter().map(|d| Data { x: d.x, y: d.y.ln() }).collect(),
      Op::Log10 => self.0.iter().map(|d| Data { x: d.x, y: d.y.log10() }).collect(),
      Op::ZScoreMean(period) => self.z_score_mean(*period),
      Op::ZScoreMedian(period) => self.z_score_median(*period),
    }
  }

  pub fn len(&self) -> usize {
    self.0.len()
  }

  pub fn is_empty(&self) -> bool {
    self.0.is_empty()
  }

  pub fn min_x(&self) -> f64 {
    self.0.iter().map(|d| d.y).fold(f64::INFINITY, |a, b| a.min(b))
  }

  pub fn max_x(&self) -> f64 {
    self.0.iter().map(|d| d.y).fold(f64::NEG_INFINITY, |a, b| a.max(b))
  }

  pub fn min_y(&self) -> f64 {
    self.0.iter().map(|d| d.y).fold(f64::INFINITY, |a, b| a.min(b))
  }

  pub fn max_y(&self) -> f64 {
    self.0.iter().map(|d| d.y).fold(f64::NEG_INFINITY, |a, b| a.max(b))
  }

  pub fn mode(&self) -> f64 {
    let mut data = self.0.clone();
    data.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap());
    let mut max_count = 0;
    let mut mode = 0.0;
    let mut count = 0;
    let mut prev = 0.0;
    for d in data {
      if d.y == prev {
        count += 1;
      } else {
        count = 1;
        prev = d.y;
      }
      if count > max_count {
        max_count = count;
        mode = d.y;
      }
    }
    mode
  }

  pub fn median(data: &[Data]) -> f64 {
    let mut data = data.to_vec();
    data.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap());
    let mid = data.len() / 2;
    if data.len() % 2 == 0 {
      (data[mid].y + data[mid - 1].y) / 2.0
    } else {
      data[mid].y
    }
  }

  pub fn mean(data: &[Data]) -> f64 {
    let sum: f64 = data.iter().map(|d| d.y).sum();
    sum / data.len() as f64
  }

  pub fn z_score_mean(&self, period: usize) -> Vec<Data> {
    let mut z_scores = Vec::new();
    for window in self.0.windows(period) {
      let mean = Self::mean(window);
      let std_dev = (window.iter().map(|x| (x.y - mean).powi(2)).sum::<f64>() / window.len() as f64).sqrt();

      // Compute Z-score for the last element in the window
      let last = window.last().unwrap().clone();
      let y = (last.y - mean) / std_dev;
      z_scores.push(Data {
        x: last.x,
        y
      });
    }
    z_scores
  }
  
  
  pub fn z_score_median(&self, period: usize) -> Vec<Data> {
    let mut z_scores = Vec::new();
    for window in self.0.windows(period) {
      let median = Self::median(window);
      let std_dev = (window.iter().map(|x| (x.y - median).powi(2)).sum::<f64>() / window.len() as f64).sqrt();

      // Compute Z-score for the last element in the window
      let last = window.last().unwrap().clone();
      let y = (last.y - median) / std_dev;
      z_scores.push(Data {
        x: last.x,
        y
      });
    }
    z_scores
  }
}