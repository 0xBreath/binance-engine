/// This library is a translation of Hurst Exponent calculation from Python to Rust:
/// @[Introduction to the Hurst exponent — with code in Python](https://towardsdatascience.com/introduction-to-the-hurst-exponent-with-code-in-python-4da0414ca52e)
pub fn hurst(series: &[f64], max_lags: u32) -> anyhow::Result<f64> {
  let lags = (2..max_lags).collect::<Vec<u32>>();
  let tau = lags
    .iter()
    .map(|lag| {
      let lag = *lag as usize;
      let series_lagged = series[lag..].to_vec();
      let series = series[..(series.len() - lag)].to_vec();
      let diff = series_lagged
        .iter()
        .zip(series.iter())
        .map(|(a, b)| a - b)
        .collect::<Vec<f64>>();
      std_dev(&diff).unwrap()
    })
    .collect::<Vec<f64>>();
  // np.log(lags)
  let lags_log = lags
    .iter()
    .map(|lag| (*lag as f64).ln())
    .collect::<Vec<f64>>();
  // np.log(tau)
  let tau_log = tau.iter().map(|tau| tau.ln()).collect::<Vec<f64>>();
  // reg = np.polyfit(np.log(lags), np.log(tau), 1)
  let reg: (f64, f64) = linreg::linear_regression(&lags_log, &tau_log)
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
  let h = reg.0;
  let _c = reg.1;
  Ok(h)
}

/// ZScore
/// Calculates the ZScore given a spread
pub fn rolling_zscore(series: &Vec<f64>, window: usize) -> Result<Vec<f64>, Box<dyn std::error::Error>> {
  let mut z_scores: Vec<f64> = vec![0.0; window]; // Padding with 0.0 for the first (window) elements

  // Guard: Ensure correct window size
  if window > series.len() {
    return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Window size is greater than vector length")));
  }

  // Calculate z-scores for each window
  for i in window..series.len() {
    let window_data: &[f64] = &series[i-window..i];
    let mean: f64 = window_data.iter().sum::<f64>() / window_data.len() as f64;
    let var: f64 = window_data.iter().map(|&val| (val - mean).powi(2)).sum::<f64>() / (window_data.len()-1) as f64;
    let std_dev: f64 = var.sqrt();
    if std_dev == 0.0 {
      return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Standard deviation is zero")));
    }
    let z_score = (series[i] - mean) / std_dev;
    z_scores.push(z_score);
  }
  Ok(z_scores)
}

fn mean(data: &[f64]) -> Option<f64> {
  let sum = data.iter().sum::<f64>();
  let count = data.len();
  match count {
    positive if positive > 0 => Some(sum / count as f64),
    _ => None,
  }
}

fn std_dev(data: &[f64]) -> Option<f64> {
  match (mean(data), data.len()) {
    (Some(data_mean), count) if count > 0 => {
      let variance = data
        .iter()
        .map(|value| {
          let diff = data_mean - *value;

          diff * diff
        })
        .sum::<f64>()
        / count as f64;

      Some(variance.sqrt())
    }
    _ => None,
  }
}