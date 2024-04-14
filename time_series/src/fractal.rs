use crate::*;
use std::fmt::{Display, Formatter};
use crate::backtest::{Direction, ReversalType};


#[derive(Debug, Clone)]
pub struct Pivot {
    pub candle: Candle,
    pub reversal_type: ReversalType,
}

#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub enum Timeframe {
    Min1,
    Min5,
    Min15,
    Hour,
    Hour4,
    Day,
    Month
}

impl Display for Timeframe {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Timeframe::Min1 => write!(f, "1m"),
            Timeframe::Min5 => write!(f, "5m"),
            Timeframe::Min15 => write!(f, "15m"),
            Timeframe::Hour => write!(f, "1h"),
            Timeframe::Hour4 => write!(f, "4h"),
            Timeframe::Day => write!(f, "1d"),
            Timeframe::Month => write!(f, "1M"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TimeSeries {
    pub series: TickerData,
    pub timeframe: Timeframe,
}

/// Price-Time Vector
#[derive(Debug, Clone)]
pub struct PriceTimeVector {
    /// First pivot candle
    pub first_pivot: Pivot,
    #[allow(dead_code)]
    /// Second pivot candle
    pub second_pivot: Pivot,
    /// UNIX time between pivots
    pub unix_time_diff: f64,
    /// Price difference between pivots
    pub price_pct_diff: f64,
    /// Timeframe of PTV
    pub timeframe: Timeframe,
    /// Direction of price movement between pivots
    pub direction: Direction,
}

#[derive(Debug, Clone, Default)]
pub struct FractalsFound {
    #[allow(dead_code)]
    current_points: Vec<PriceTimeVector>,
    #[allow(dead_code)]
    past_fractals: Vec<Vec<PriceTimeVector>>,
}

#[derive(Debug, Clone)]
pub struct Fractal {
    /// Pivot is higher/lower than these bars to the left
    pub left_bars: usize,
    /// Pivot is higher/lower than these bars to the right
    pub right_bars: usize,
    /// Factor time in search for fractals
    pub use_time: bool,
    /// NUmber of pivots into the past to use for searching for fractals
    /// Used to collect backtesting data, default to 0
    pub pivots_back: usize,
    /// Number of pivots to compare to search for fractals
    pub num_compare: usize,
    /// Number of pivots to forecast; includes `num_compare`
    pub num_forecast: usize,
}

impl Fractal {
    pub fn new(
        left_bars: usize,
        right_bars: usize,
        use_time: bool,
        pivots_back: usize,
        num_compare: usize,
        num_forecast: usize
    ) -> Self {
        Self {
            left_bars,
            right_bars,
            use_time,
            pivots_back,
            num_compare,
            num_forecast
        }
    }

    fn ptv(first_pivot: Pivot, second_pivot: Pivot, timeframe: Timeframe) -> PriceTimeVector {
        let unix_time_diff =
          (1.0 + ((second_pivot.candle.date.to_unix() as f64 - first_pivot.candle.date.to_unix() as f64) / first_pivot.candle.date.to_unix() as f64)) * 100.0;
        println!("unix_time_diff = {}", unix_time_diff);
        let price_pct_diff =
          (1.0 + ((second_pivot.candle.close - first_pivot.candle.close) / first_pivot.candle.close)) * 100.0;
        println!("price_pct_diff = {}", price_pct_diff);
        let direction = if second_pivot.candle.close > first_pivot.candle.close {
            Direction::Up
        } else {
            Direction::Down
        };
        PriceTimeVector {
            first_pivot,
            second_pivot,
            unix_time_diff,
            price_pct_diff,
            timeframe,
            direction,
        }
    }

    /// True is fractals, false is not fractals
    fn compare_price_dimension(curr: &PriceTimeVector, past: &PriceTimeVector) -> bool {
        ((curr.price_pct_diff / past.price_pct_diff) - 1.0).abs() < 0.05
    }

    /// True is fractals, false is not fractals
    fn compare_time_dimension(curr: &PriceTimeVector, past: &PriceTimeVector) -> bool {
        ((curr.unix_time_diff / past.unix_time_diff) - 1.0).abs() < 0.05
    }

    /// Iterate both PriceTimeVector for past and present.
    /// Check if time dimensions are beyond 2 stdev (<0.05) of each other.
    fn fractal_time_dimension(
        &self,
        curr: &[PriceTimeVector],
        past: &[PriceTimeVector],
    ) -> std::io::Result<bool> {
        if curr.len() != past.len() {
            println!("curr.len() = {}", curr.len());
            println!("past.len() = {}", past.len());
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "curr ptvs != past ptvs",
            ));
        }
        let mut is_fractal = false;
        for (index, (curr_ptv, past_ptv)) in curr.iter().zip(past.iter()).enumerate() {
            if index >= self.num_compare {
                return Ok(is_fractal);
            }
            match Self::compare_time_dimension(curr_ptv, past_ptv) {
                true => is_fractal = true,
                false => return Ok(false),
            }
        }
        Ok(is_fractal)
    }

    /// Iterate both PriceTimeVector for past and present.
    /// Check if price dimensions are beyond 2 stdev (<0.05) of each other.
    fn fractal_price_dimension(
        &self,
        curr: &[PriceTimeVector],
        past: &[PriceTimeVector],
    ) -> std::io::Result<bool> {
        if curr.len() != past.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "curr ptvs != past ptvs",
            ));
        }
        let mut is_fractal = false;
        for (index, (curr_ptv, past_ptv)) in curr.iter().zip(past.iter()).enumerate() {
            if index >= self.num_compare {
                return Ok(is_fractal);
            }
            match Self::compare_price_dimension(curr_ptv, past_ptv) {
                true => is_fractal = true,
                false => return Ok(false),
            }
        }
        Ok(is_fractal)
    }

    fn directions_match(
        &self,
        ptvs_1: &[PriceTimeVector],
        ptvs_2: &[PriceTimeVector],
    ) -> std::io::Result<bool> {
        if ptvs_1.len() != ptvs_2.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "ptvs_1.len() != ptvs_2.len()",
            ));
        }
        // compare if general direction between PTVs on both PriceTimeVectors is the same
        let mut directions_match = true;
        for i in 0..self.num_compare {
            if ptvs_1[i].direction != ptvs_2[i].direction {
                directions_match = false;
            }
        }

        // compare if each pivot point is higher/lower than the first pivot point
        let curr_first_pivot = ptvs_1[0].first_pivot.clone();
        let pivots_relative_to_first_curr = ptvs_1
            .iter()
            .map(|ptv| ptv.first_pivot.candle.close > curr_first_pivot.candle.close)
            .collect::<Vec<bool>>();
        let past_first_pivot = ptvs_2[0].first_pivot.clone();
        let pivots_relative_to_first_past = ptvs_2
            .iter()
            .map(|ptv| ptv.first_pivot.candle.close > past_first_pivot.candle.close)
            .collect::<Vec<bool>>();
        if pivots_relative_to_first_curr.len() != pivots_relative_to_first_past.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "pivots_relative_to_first_curr.len() != pivots_relative_to_first_past.len()",
            ));
        }
        // check each value is equal to each other in pivots_relative_to_first_curr and pivots_relative_to_first_past
        for i in 0..self.num_compare {
            if pivots_relative_to_first_curr[i] != pivots_relative_to_first_past[i] {
                directions_match = false;
            }
        }

        // compare if each Pivot has the same reversal_type
        let curr_reversal_types = ptvs_1
            .iter()
            .map(|ptv| ptv.first_pivot.reversal_type.clone())
            .collect::<Vec<ReversalType>>();
        let past_reversal_types = ptvs_2
            .iter()
            .map(|ptv| ptv.first_pivot.reversal_type.clone())
            .collect::<Vec<ReversalType>>();
        if curr_reversal_types.len() != past_reversal_types.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "curr_reversal_types.len() != past_reversal_types.len()",
            ));
        }
        for i in 0..self.num_compare {
            if curr_reversal_types[i] != past_reversal_types[i] {
                directions_match = false;
            }
        }
        Ok(directions_match)
    }

    /// Compare time and price dimensions of past and present PriceTimeVectors.
    /// If all points up to `num_compare` are proportional and directions are the same, fractal is found.
    fn fractal_found(&self, curr: &[PriceTimeVector], past: &[PriceTimeVector]) -> bool {
        let frac_time = self
            .fractal_time_dimension(curr, past)
            .expect("fractal_time_dimension");
        let frac_price = self
            .fractal_price_dimension(curr, past)
            .expect("fractal_price_dimension");
        let dir_match = self.directions_match(curr, past).expect("directions_match");
        if self.use_time {
            frac_price && dir_match && frac_time
        } else {
            frac_price && dir_match
        }
    }

    pub fn fractals(&self, all_time_series: Vec<TimeSeries>) -> Vec<(Vec<PriceTimeVector>, Vec<PriceTimeVector>)> {
        let mut all_timeframe_ptvs = Vec::<Vec<PriceTimeVector>>::new();
        let mut latest_ptvs = Vec::<Vec<PriceTimeVector>>::new();
        // iterate each time_series in all_time_series
        for time_series in all_time_series.iter() {
            // identify pivot highs and pivot lows, sort by Time oldest to newest as Vec<Candle>
            let pivot_lows = time_series
                .series
                .pivot_lows(self.left_bars, self.right_bars)
                .into_iter().map(|candle| Pivot{candle, reversal_type: ReversalType::Low})
                .collect::<Vec<Pivot>>();
            let pivot_highs = time_series
                .series
                .pivot_highs(self.left_bars, self.right_bars)
                .into_iter().map(|candle| Pivot{candle, reversal_type: ReversalType::High})
                .collect::<Vec<Pivot>>();
            let mut pivots = pivot_lows
                .into_iter()
                .chain(pivot_highs.into_iter())
                .collect::<Vec<Pivot>>();
            pivots.sort_by(|a, b| a.candle.date.partial_cmp(&b.candle.date).unwrap());

            // compute PTV between each pivot, store in Vec<PriceTimeVector>
            let mut ptvs = Vec::new();
            for i in 0..pivots.len() - 1 {
                ptvs.push(Self::ptv(
                    pivots[i].clone(),
                    pivots[i + 1].clone(),
                    time_series.timeframe.clone(),
                ));
            }
            all_timeframe_ptvs.push(ptvs);

            // find 3 most recent PTVs from today on this time_series timeframe
            let mut recent_ptvs = Vec::new();
            let start_index = pivots.len() - 1 - self.num_forecast - self.pivots_back;
            let end_index = pivots.len() - 1 - self.pivots_back;
            for i in start_index..end_index {
                let ptv = Self::ptv(
                    pivots[i].clone(),
                    pivots[i + 1].clone(),
                    time_series.timeframe.clone(),
                );
                recent_ptvs.push(ptv);
            }
            latest_ptvs.push(recent_ptvs);
        }

        // iterate each timeframe vector of PTVs
        let mut fractals = Vec::new();
        for timeframe_ptvs in all_timeframe_ptvs {
            for curr_ptvs in latest_ptvs.iter() {
                for i in 0..(timeframe_ptvs.len() - 1 - self.num_forecast) {
                    if timeframe_ptvs.len() < self.num_forecast {
                        break;
                    }
                    let compare_ptvs = &timeframe_ptvs[i..i + self.num_forecast];
                    if self.fractal_found(curr_ptvs, compare_ptvs) {
                        fractals.push((curr_ptvs.to_vec(), compare_ptvs.to_vec()));
                    }
                }
            }
        }
        // remove fractals if curr_ptv[0] and past_ptv[0] have the same date and timeframe
        fractals.into_iter().filter(|(curr, past)| {
            curr[0].first_pivot.candle.date != past[0].first_pivot.candle.date && curr[0].timeframe != past[0].timeframe
        }).collect::<Vec<_>>()
    }
}
