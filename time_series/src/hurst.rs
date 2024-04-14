#[derive(Debug)]
pub enum HurstError {
    LinearRegression(String),
    Polyfit(String),
}

pub type HurstResult<T> = Result<T, HurstError>;

/// This library is a translation of Hurst Exponent calculation from Python to Rust:
/// @[Introduction to the Hurst exponent — with code in Python](https://towardsdatascience.com/introduction-to-the-hurst-exponent-with-code-in-python-4da0414ca52e)
pub fn hurst(series: &[f64], max_lags: u32) -> HurstResult<f64> {
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
        .map_err(|e| HurstError::LinearRegression(e.to_string()))?;
    let h = reg.0;
    let _c = reg.1;
    Ok(h)
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
