use crate::{Candle, MarketStructure, MarketStructureError, Time, TimeError};
use chrono::Duration;
use log::debug;
use rayon::prelude::*;
use std::f64::consts::PI;
use std::fmt;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Copy)]
pub enum CycleTimeframe {
    Hour,
    Day,
    Week,
    Month,
}

impl CycleTimeframe {
    pub fn timeframe_to_period_minutes(&self, period: f32) -> u64 {
        match self {
            CycleTimeframe::Hour => (period * 60.0).round() as u64,
            CycleTimeframe::Day => (period * 24.0 * 60.0).round() as u64,
            CycleTimeframe::Week => (period * 24.0 * 60.0 * 7.0).round() as u64,
            CycleTimeframe::Month => (period * 24.0 * 60.0 * 30.0).round() as u64,
        }
    }

    pub fn period_minutes_to_timeframe(&self, period: u64) -> f32 {
        let period = period as f32;
        match self {
            CycleTimeframe::Hour => period / 60.0,
            CycleTimeframe::Day => period / 24.0 / 60.0,
            CycleTimeframe::Week => period / 24.0 / 60.0 / 7.0,
            CycleTimeframe::Month => period / 24.0 / 60.0 / 30.0,
        }
    }

    pub fn to_str(&self) -> &str {
        match self {
            CycleTimeframe::Hour => "Hour",
            CycleTimeframe::Day => "Day",
            CycleTimeframe::Week => "Week",
            CycleTimeframe::Month => "Month",
        }
    }
}

#[derive(Debug)]
pub enum CycleError {
    TimeError(TimeError),
    EmptyCycle,
    CyclesNotSameDates,
    TimeSeriesDatesBeyondCycle,
    EmptySeries,
    EndDateBeforeStartDate,
    CustomError(std::io::Error),
    EmptyMean,
    EmptyMedian,
    EmptyStdDev,
    EmptyZScore,
    MarketStructureError(MarketStructureError),
}

impl Display for CycleError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            CycleError::TimeError(e) => write!(f, "Time error: {}", e),
            CycleError::EmptyCycle => write!(f, "Empty cycle"),
            CycleError::CyclesNotSameDates => write!(f, "Cycles do not have same dates"),
            CycleError::TimeSeriesDatesBeyondCycle => {
                write!(f, "Time series dates beyond cycle dates")
            }
            CycleError::EmptySeries => write!(f, "Empty series"),
            CycleError::EndDateBeforeStartDate => write!(f, "End date before start date"),
            CycleError::CustomError(e) => write!(f, "Custom error: {}", e),
            CycleError::EmptyMean => write!(f, "Empty mean"),
            CycleError::EmptyMedian => write!(f, "Empty median"),
            CycleError::EmptyStdDev => write!(f, "Empty standard deviation"),
            CycleError::EmptyZScore => write!(f, "Empty zscore"),
            CycleError::MarketStructureError(e) => write!(f, "Market structure error: {}", e),
        }
    }
}

pub type CycleResult<T> = Result<T, CycleError>;

#[derive(Debug, Clone)]
pub struct SineCycle {
    pub start_date: Time,
    pub end_date: Time,
    pub cycle_period_minutes: u64,
    pub timeframe: CycleTimeframe,
}

#[derive(Debug, Clone)]
pub struct CyclePoint {
    pub x: Time,
    pub y: f64,
}

// cycles have different Y values, so compare X values (dates) only
impl PartialEq for CyclePoint {
    fn eq(&self, other: &Self) -> bool {
        self.x == other.x
    }
}

#[derive(Debug, Clone)]
pub struct CycleCorrelation {
    pub correlated: usize,
    pub total: usize,
    pub correlation: f64,
}

impl SineCycle {
    pub fn new(
        start_date: Time,
        end_date: Time,
        cycle_period_minutes: u64,
        timeframe: CycleTimeframe,
    ) -> Self {
        Self {
            start_date,
            end_date,
            cycle_period_minutes,
            timeframe,
        }
    }

    /// TODO: compute wave function amplitude based on Self::sine_wave price magnitude ?
    pub fn wave_function(&self, x: &Time) -> CycleResult<f64> {
        let period = x
            .diff_minutes(&self.start_date)
            .map_err(CycleError::TimeError)? as f64;

        // phase shift the sine wave to start at trough (-90 degrees)
        let phase_shift = -PI / 2.0;

        let omega = 2.0 * PI / self.cycle_period_minutes as f64;
        let sine = (phase_shift + period * omega).sin();
        Ok(sine)
    }

    pub fn sine_wave(&self) -> CycleResult<Vec<CyclePoint>> {
        let start = self
            .start_date
            .to_datetime()
            .map_err(CycleError::TimeError)?;
        let end = self.end_date.to_datetime().map_err(CycleError::TimeError)?;

        let mut result = Vec::new();
        let mut curr = start;
        let time_step = Duration::minutes(1);
        while curr < end {
            let x = Time::from_datetime(curr);
            let y = self.wave_function(&x)?;
            result.push(CyclePoint { x, y });
            curr += time_step;
        }
        Ok(result)
    }

    /// Measure sine wave correlation to time series by using comparing slope of Candle to SineCycle slope
    pub fn series_correlation(&self, series: &MarketStructure) -> CycleResult<CycleCorrelation> {
        // start cycle analysis at first major low of time series
        let first_low = series
            .first_low()
            .map_err(CycleError::MarketStructureError)?;
        match series.candles.last() {
            Some(last) => {
                // assert time series dates are less than or equal to cycle dates
                if first_low.candle.date < self.start_date || self.end_date < last.date {
                    return Err(CycleError::TimeSeriesDatesBeyondCycle);
                }
            }
            _ => return Err(CycleError::EmptySeries),
        }

        // filter series.candles from self.start_date to self.end_date
        let candles = series
            .candles
            .iter()
            .filter(|c| self.start_date <= c.date && c.date <= self.end_date)
            .collect::<Vec<&Candle>>();

        let mut correlated = 0;
        let mut total = 0;
        for candles in candles.windows(2) {
            if let (Some(prev), Some(curr)) = (candles.get(0), candles.get(1)) {
                // compute slope of price change
                let price_slope = curr.close - prev.close > 0.0;
                // compute slope of sine wave
                let wave_slope =
                    self.wave_function(&curr.date)? - self.wave_function(&prev.date)? > 0.0;
                // compare slopes, correlated if sloped are both positive or negative
                if price_slope == wave_slope {
                    correlated += 1;
                }
                total += 1;
            }
        }

        Ok(CycleCorrelation {
            correlated,
            total,
            correlation: correlated as f64 / total as f64,
        })
    }
}

pub type CycleHarmonics = Vec<u32>;

#[derive(Debug, Clone)]
pub struct CompositeCycle {
    pub start_date: Time,
    pub end_date: Time,
    pub sine_cycles: Vec<SineCycle>,
    pub composite: Vec<CyclePoint>,
}

impl CompositeCycle {
    pub fn composite_wave(cycles: &[SineCycle]) -> CycleResult<Vec<CyclePoint>> {
        let first_cycle = match cycles.first() {
            Some(first) => first,
            None => return Err(CycleError::EmptyCycle),
        };
        // all cycles should share the same dates
        for cycle in cycles.iter() {
            if first_cycle.start_date != cycle.start_date || first_cycle.end_date != cycle.end_date
            {
                return Err(CycleError::CyclesNotSameDates);
            }
        }

        // compute composite wave by summing all sine wave functions at each date
        let mut composite: Vec<CyclePoint> = Vec::new();
        let first_sine = first_cycle.sine_wave()?;
        for date in first_sine.iter().map(|c| c.x) {
            // sum sine wave function of each cycle
            let mut components: Vec<f64> = Vec::new();
            for cycle in cycles.iter() {
                // TODO: handle amplitude of each wave function by returning (amplitude, equation) tuple ?
                let wave_function = cycle.wave_function(&date)?;
                components.push(wave_function);
            }
            // composite point is sum of sine wave functions
            let point: f64 = components.into_iter().sum();
            composite.push(CyclePoint { x: date, y: point });
        }
        Ok(composite)
    }

    pub fn cycle_combinations(
        start_date: &Time,
        end_date: &Time,
        // period in timeframe units (e.g. 1 month cycle)
        cycle_periods: &[u64],
        cycle_timeframe: CycleTimeframe,
    ) -> CycleResult<Vec<CompositeCycle>> {
        let mut composites: Vec<CompositeCycle> = Vec::new();
        for k in 1..=cycle_periods.len() {
            // cycle combinations of length k
            let combs = Self::combinations_inner(cycle_periods, k);

            let pre = std::time::SystemTime::now();
            // parallelize using rayon
            let result: Vec<CompositeCycle> = combs
                .par_iter()
                .map(|comb| {
                    let pre = std::time::SystemTime::now();
                    // map harmonic cycles to SineCycle
                    let sine_cycles = comb
                        .iter()
                        .map(|period| SineCycle {
                            start_date: *start_date,
                            end_date: *end_date,
                            cycle_period_minutes: cycle_timeframe
                                .timeframe_to_period_minutes(*period as f32),
                            timeframe: cycle_timeframe.clone(),
                        })
                        .collect::<Vec<SineCycle>>();

                    // compute composite wave for these harmonic cycles
                    let composite = Self::composite_wave(&sine_cycles).unwrap();
                    debug!(
                        "comb: {:?}, time: {}ms",
                        comb,
                        pre.elapsed().unwrap().as_millis()
                    );
                    CompositeCycle {
                        start_date: *start_date,
                        end_date: *end_date,
                        sine_cycles,
                        composite,
                    }
                })
                .collect();
            composites.extend(result);
            debug!("post rayon: {}ms", pre.elapsed().unwrap().as_millis());
        }
        Ok(composites)
    }

    fn combinations_inner(slice: &[u64], k: usize) -> Vec<Vec<u64>> {
        if k == 0 {
            return vec![Vec::new()];
        }
        if slice.len() < k {
            return Vec::new();
        }
        let mut result = Vec::new();
        let first = slice[0];
        let rest = &slice[1..];
        for mut combination in Self::combinations_inner(rest, k - 1) {
            combination.insert(0, first);
            result.push(combination);
        }
        result.extend(Self::combinations_inner(rest, k));
        result
    }

    /// Measure sine wave correlation to time series by using comparing slope of Candle to SineCycle slope
    pub fn series_correlation(&self, series: &MarketStructure) -> CycleResult<CycleCorrelation> {
        // start cycle analysis at first major low of time series
        let first_low = series
            .first_low()
            .map_err(CycleError::MarketStructureError)?;
        let (start, end) = match series.candles.last() {
            Some(last) => {
                // assert time series dates are less than or equal to cycle dates
                if first_low.candle.date < self.start_date || self.end_date < last.date {
                    return Err(CycleError::TimeSeriesDatesBeyondCycle);
                }
                (first_low.candle, last)
            }
            _ => return Err(CycleError::EmptySeries),
        };

        // filter series.candles from self.start_date to self.end_date
        let candles = series
            .candles
            .iter()
            .filter(|c| start.date <= c.date && c.date <= end.date)
            .collect::<Vec<&Candle>>();

        let mut correlated = 0;
        let mut total = 0;
        for candles in candles.windows(2) {
            if let (Some(prev), Some(curr)) = (candles.get(0), candles.get(1)) {
                // compute slope of price change
                let price_slope = curr.close - prev.close > 0.0;
                // compute slope of composite wave
                let prev_cycle = self.composite_function(&prev.date)?;
                let curr_cycle = self.composite_function(&curr.date)?;
                let wave_slope = curr_cycle - prev_cycle > 0.0;
                // compare slopes, correlated if sloped are both positive or negative
                if price_slope == wave_slope {
                    correlated += 1;
                }
                total += 1;
            }
        }

        Ok(CycleCorrelation {
            correlated,
            total,
            correlation: correlated as f64 / total as f64,
        })
    }

    fn composite_function(&self, x: &Time) -> CycleResult<f64> {
        let mut composite = 0.0;
        for cycle in self.sine_cycles.iter() {
            composite += cycle.wave_function(x)?;
        }
        Ok(composite)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;
    use log::info;

    #[test]
    fn sine_wave() -> CycleResult<()> {
        let start_date = Time::new(2020, &Month::from_num(1), &Day::from_num(1), None, None);
        // sine wave of 5 week cycle
        let cycle = SineCycle::new(
            start_date,
            start_date.delta_date(1),
            60 * 24 * 7 * 5,
            CycleTimeframe::Week,
        );
        let wave = cycle.sine_wave()?;
        for point in wave.iter().take(10) {
            info!("x: {}, y: {}", point.x.to_string(), point.y);
        }
        Ok(())
    }
}
