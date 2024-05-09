#![allow(dead_code)]
#![allow(clippy::unnecessary_cast)]

use std::collections::HashSet;
use crate::{Candle, Data, Dataset, Time, X, Y};
use std::fs::File;
use std::path::PathBuf;
use std::str::FromStr;

pub struct CsvSeries {
  pub candles: Vec<Candle>,
}

pub struct Dataframe;

impl Dataframe {
  /// Read candles from CSV file.
  /// Handles duplicate candles and sorts candles by date.
  /// Expects date of candle to be in UNIX timestamp format.
  /// CSV format: date,open,high,low,close,volume
  pub fn csv_series(csv_path: &PathBuf, start_time: Option<Time>, end_time: Option<Time>, _ticker: String) -> anyhow::Result<CsvSeries> {
    let file_buffer = File::open(csv_path)?;
    let mut csv = csv::Reader::from_reader(file_buffer);

    let mut headers = vec![];
    if let Ok(result) = csv.headers() {
      for header in result {
        headers.push(String::from(header));
      }
    }

    let mut candles = vec![];

    for record in csv.records().flatten() {
      let date = Time::from_unix(
        record[0]
          .parse::<i64>()
          .expect("failed to parse candle UNIX timestamp into i64"),
      );
      let volume = None;
      let candle = Candle {
        date,
        open: f64::from_str(&record[1])?,
        high: f64::from_str(&record[2])?,
        low: f64::from_str(&record[3])?,
        close: f64::from_str(&record[4])?,
        volume,
      };
      candles.push(candle);
    }
    // only take candles greater than a timestamp
    candles.retain(|candle| {
      match (start_time, end_time) {
        (Some(start), Some(end)) => {
          candle.date.to_unix_ms() > start.to_unix_ms() && candle.date.to_unix_ms() < end.to_unix_ms()
        },
        (Some(start), None) => {
          candle.date.to_unix_ms() > start.to_unix_ms()
        },
        (None, Some(end)) => {
          candle.date.to_unix_ms() < end.to_unix_ms()
        },
        (None, None) => true
      }
    });

    Ok(CsvSeries {
      candles,
    })
  }

  pub fn align_pair_series(x: &mut Vec<Candle>, y: &mut Vec<Candle>) -> anyhow::Result<()> {
    // retain the overlapping dates between the two time series
    // Step 1: Create sets of timestamps from both vectors
    let x_dates: HashSet<i64> = x.iter().map(|c| c.date.to_unix_ms()).collect();
    let y_dates: HashSet<i64> = y.iter().map(|c| c.date.to_unix_ms()).collect();
    // Step 2: Find the intersection of both timestamp sets
    let common_timestamps: HashSet<&i64> = x_dates.intersection(&y_dates).collect();
    // Step 3: Filter each vector to keep only the common timestamps
    x.retain(|c| common_timestamps.contains(&c.date.to_unix_ms()));
    y.retain(|c| common_timestamps.contains(&c.date.to_unix_ms()));
    // Step 4: Sort both vectors by timestamp to ensure they are aligned
    // earliest point in time is 0th index, latest point in time is Nth index
    x.sort_by_key(|c| c.date.to_unix_ms());
    y.sort_by_key(|c| c.date.to_unix_ms());
    Ok(())
  }

  /// Redefine each price point as a percentage change relative to the starting price.
  pub fn normalize_series<T: X + Y>(series: &[T]) -> anyhow::Result<Dataset<i64, f64>> {
    let mut series = series.to_vec();
    series.sort_by_key(|c| c.x());
    let d_0 = series.first().unwrap().clone();
    let x: Dataset<i64, f64> = Dataset::new(series.iter().map(|d| Data {
      x: d.x(),
      y: (d.y() / d_0.y() - 1.0) * 100.0
    }).collect());
    Ok(x)
  }

  pub fn lagged_spread_series<T: X + Y>(series: &[T]) -> anyhow::Result<Dataset<i64, f64>> {
    let mut series = series.to_vec();
    series.sort_by_key(|c| c.x());
    let spread: Vec<Data<i64, f64>> = series.windows(2).map(|x| Data {
      x: x[1].x(),
      y: x[1].y() - x[0].y()
    }).collect();
    Ok(Dataset::new(spread))
  }
}