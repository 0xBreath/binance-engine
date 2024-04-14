use crate::{
    Backtest, Candle, CycleError, Direction, Order, ReversalType, TickerData, TickerDataError,
    Time, TimeError, Trade, TrailingStopType,
};
use std::error::Error;
use std::fs::File;

use chrono::{Duration, Local, NaiveDate, TimeZone};
use log::{debug, info};
use plotters::prelude::*;
use std::io::Write;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub enum PFSError {
    CycleError(CycleError),
    TimeError(TimeError),
    TickerDataError(TickerDataError),
    BacktestEmpty,
    CustomError(std::io::Error),
}

impl std::fmt::Display for PFSError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PFSError::CycleError(e) => write!(f, "Cycle error: {}", e),
            PFSError::TimeError(e) => write!(f, "Time error: {}", e),
            PFSError::TickerDataError(e) => write!(f, "Ticker data error: {}", e),
            PFSError::BacktestEmpty => write!(f, "Backtest is empty"),
            PFSError::CustomError(e) => write!(f, "Custom error: {}", e),
        }
    }
}

pub type PFSResult<T> = Result<T, PFSError>;

#[derive(Debug, Clone)]
/// Backtest correlation
pub struct IndividualPFSCorrelation {
    pub cycle: u32,
    pub hits: u32,
    pub total: u32,
    pub pct_correlation: f64,
}

#[derive(Debug, Clone)]
pub struct ConfluentPFSCorrelation {
    pub cycles: Vec<u32>,
    pub events: Vec<ConfluentPFSEvent>,
    pub hits: u32,
    pub total: u32,
    pub pct_correlation: f64,
}

#[derive(Debug, Clone)]
pub struct ConfluentPFSEvent {
    pub date: Time,
    pub cycles: Option<Vec<u32>>,
    pub reversal: Option<ReversalType>,
    pub direction: Option<Direction>,
}

#[derive(Debug, Clone, Copy)]
pub enum PFSTimeframe {
    Minute,
    Hour,
    Day,
    Month,
    Year,
}

/// Polarity Factor System
#[derive(Debug, Clone)]
pub struct PFS {
    pub date: Time,
    pub value: f64,
    pub cycle: u32,
}

impl PFS {
    pub fn new(date: Time, value: f64, cycle: u32) -> Self {
        Self { date, value, cycle }
    }
}

pub struct PlotPFS {
    pub start_date: Time,
    pub end_date: Time,
}

impl PlotPFS {
    pub fn new(start_date: Time, end_date: Time) -> Self {
        Self {
            start_date,
            end_date,
        }
    }

    /// Compute PFS based on monthly cycles
    /// e.g. PFS 20 is the average percent change in price every 20 months into the past
    pub fn pfs_months(
        start_date: Time,
        end_date: Time,
        ticker_data: &TickerData,
        cycle_months: u32,
    ) -> Vec<PFS> {
        let mut monthly_pfs = Vec::<PFS>::new();

        // compute number of cycles possible in candle history
        let earliest_candle_month = ticker_data.earliest_date().month;
        let earliest_candle_year = ticker_data.earliest_date().year;
        let latest_candle_month = ticker_data.latest_date().month;
        let latest_candle_year = ticker_data.latest_date().year;
        let months = ((latest_candle_year - earliest_candle_year) * 12) as u32;
        let num_cycles =
            (latest_candle_month.to_num() + (12 - earliest_candle_month.to_num()) + months) as i32
                / cycle_months as i32;

        let time_period = start_date.time_period(&end_date);
        for date in time_period.iter() {
            // PFS for this date
            let mut pfs = (100.0, 1);
            // iterate possible cycles in candle history
            for cycle in 1..num_cycles + 1 {
                // find candle X cycles back
                for (index, candle) in ticker_data.candles.iter().enumerate() {
                    if index == 0 {
                        continue;
                    }
                    // used to compute percent change between candles
                    let prev_candle = ticker_data
                        .candles
                        .get(index - 1)
                        .expect("Failed to get previous candle");
                    // candle X cycles back
                    let cycle_date = date
                        .delta_months(-(cycle_months as i32 * cycle))
                        .expect("Invalid date");
                    // if cycle_date is leap day
                    if cycle_date.month.to_num() == 2 && cycle_date.day.to_num() == 29 {
                        continue;
                    }
                    if &cycle_date < ticker_data.earliest_date() {
                        continue;
                    }
                    // found candle X cycles back
                    if prev_candle.date < cycle_date && candle.date >= cycle_date {
                        let change = candle.percent_change(prev_candle.close);
                        pfs = (pfs.0 + change, pfs.1 + 1);
                        break;
                    }
                }
            }
            monthly_pfs.push(PFS {
                date: *date,
                value: pfs.0 / pfs.1 as f64,
                cycle: cycle_months,
            });
        }
        monthly_pfs
    }

    /// Compute PFS based on daily cycles
    /// e.g. PFS 20 is the average percent change in price every 20 days into the past
    pub fn pfs_days(
        start_date: Time,
        end_date: Time,
        ticker_data: &TickerData,
        cycle_days: u32,
    ) -> PFSResult<Vec<PFS>> {
        let mut daily_pfs = Vec::<PFS>::new();

        // compute number of cycles possible in candle history
        let earliest_candle = ticker_data.earliest_date();
        let latest_candle = ticker_data.latest_date();
        let days = earliest_candle
            .diff_days(latest_candle)
            .map_err(PFSError::TimeError)?;
        let num_cycles = days / cycle_days as i64;

        let time_period = start_date.time_period(&end_date);
        for date in time_period.iter() {
            // PFS for this date
            // let mut pfs = (100.0, 1);
            let mut pfs = vec![100.0];
            // iterate possible cycles in candle history
            for cycle in 1..num_cycles + 1 {
                // candle X cycles back
                let cycle_date = date.delta_date(-(cycle_days as i64 * cycle));
                // if cycle_date is leap day
                if (cycle_date.month.to_num() == 2 && cycle_date.day.to_num() == 29)
                    || (&cycle_date < ticker_data.earliest_date())
                {
                    continue;
                }
                // find candle X cycles back
                for (index, candle) in ticker_data.candles.iter().enumerate() {
                    if index == 0 {
                        continue;
                    }
                    if candle.date > *date {
                        break;
                    }
                    // used to compute percent change between candles
                    let prev_candle = ticker_data
                        .candles
                        .get(index - 1)
                        .expect("Failed to get previous candle");
                    // found candle X cycles back
                    if prev_candle.date < cycle_date && candle.date >= cycle_date {
                        let change = candle.percent_change(prev_candle.close);
                        pfs.push(change);
                        break;
                    }
                }
            }
            let pfs_mean = pfs.iter().sum::<f64>() / pfs.len() as f64;
            daily_pfs.push(PFS {
                date: *date,
                value: pfs_mean,
                cycle: cycle_days,
            });
        }
        daily_pfs.dedup_by(|a, b| a.date == b.date);
        Ok(daily_pfs)
    }

    /// Compute PFS based on yearly cycles,
    /// e.g. PFS 20 is the average percent change in price every 20 years into the past
    pub fn pfs_years(
        start_date: Time,
        end_date: Time,
        ticker_data: &TickerData,
        cycle_years: u32,
    ) -> Vec<PFS> {
        let mut yearly_pfs = Vec::<PFS>::new();

        // compute number of cycles possible in candle history
        let earliest_candle_year = ticker_data.earliest_date().year;
        let latest_candle_year = ticker_data.latest_date().year;
        let num_cycles = (latest_candle_year - earliest_candle_year) / cycle_years as i32;

        let time_period = start_date.time_period(&end_date);
        for date in time_period.iter() {
            // PFS for this date
            let mut pfs = (100.0, 1);
            // iterate possible cycles in candle history
            for cycle in 1..num_cycles + 1 {
                // find candle X cycles back
                for (index, candle) in ticker_data.candles.iter().enumerate() {
                    if index == 0 {
                        continue;
                    }
                    // used to compute percent change between candles
                    let prev_candle = ticker_data
                        .candles
                        .get(index - 1)
                        .expect("Failed to get previous candle");
                    // candle X cycles back
                    if date.year < candle.date.year - cycle_years as i32 * cycle {
                        continue;
                    }
                    let cycle_date = Time::new(
                        date.year - cycle_years as i32 * cycle,
                        &date.month,
                        &date.day,
                        None,
                        None,
                    );
                    // if cycle_date is leap day
                    if cycle_date.month.to_num() == 2 && cycle_date.day.to_num() == 29 {
                        continue;
                    }
                    if &cycle_date < ticker_data.earliest_date() {
                        continue;
                    }
                    // found candle X cycles back
                    if prev_candle.date < cycle_date && candle.date >= cycle_date {
                        let change = candle.percent_change(prev_candle.close);
                        pfs = (pfs.0 + change, pfs.1 + 1);
                        break;
                    }
                }
            }
            yearly_pfs.push(PFS {
                date: *date,
                value: pfs.0 / pfs.1 as f64,
                cycle: cycle_years,
            });
        }
        yearly_pfs
    }

    fn find_confluent_pfs_reversal(
        &self,
        pfs_cycles: &[Vec<PFS>],
        cycles: &[u32],
        ticker_data: &TickerData,
        target_date: &Time,
    ) -> Option<ConfluentPFSEvent> {
        // find index in ticker_data.candles for target_date
        match ticker_data
            .get_candles()
            .iter()
            .position(|c| &c.date == target_date)
        {
            None => {
                debug!(
                    "Failed to find index for confluent PFS reversal target_date: {}",
                    target_date.to_string()
                );
                None
            }
            Some(target_index) => {
                if target_index == 0 || target_index == ticker_data.get_candles().len() - 1 {
                    return None;
                }
                // use previous, current, and next dates to find PFS reversal
                let prev_date = ticker_data
                    .get_candles()
                    .get(target_index - 1)
                    .expect("Failed to get previous candle")
                    .date;
                let target_date = ticker_data
                    .get_candles()
                    .get(target_index)
                    .expect("Failed to get current candle")
                    .date;
                let next_date = ticker_data
                    .get_candles()
                    .get(target_index + 1)
                    .expect("Failed to get next candle")
                    .date;

                let mut all_pfs_reversals: Vec<Option<ReversalType>> = Vec::new();
                let mut confluent_reversal: Option<ReversalType> = None;
                for pfs in pfs_cycles.iter() {
                    let prev_pfs = pfs.iter().find(|p| p.date == prev_date);
                    let target_pfs = pfs.iter().find(|p| p.date == target_date);
                    let next_pfs = pfs.iter().find(|p| p.date == next_date);
                    if let (Some(prev_pfs), Some(target_pfs), Some(next_pfs)) =
                        (prev_pfs, target_pfs, next_pfs)
                    {
                        if prev_pfs.value < target_pfs.value && target_pfs.value > next_pfs.value {
                            all_pfs_reversals.push(Some(ReversalType::High));
                        } else if prev_pfs.value > target_pfs.value
                            && target_pfs.value < next_pfs.value
                        {
                            all_pfs_reversals.push(Some(ReversalType::Low));
                        }
                    } else {
                        return None;
                    }
                }
                // determine if all PFS are highs or lows
                if all_pfs_reversals
                    .iter()
                    .all(|p| p == &Some(ReversalType::High))
                {
                    confluent_reversal = Some(ReversalType::High);
                } else if all_pfs_reversals
                    .iter()
                    .all(|p| p == &Some(ReversalType::Low))
                {
                    confluent_reversal = Some(ReversalType::Low);
                }

                Some(ConfluentPFSEvent {
                    date: target_date,
                    cycles: Some(cycles.to_vec()),
                    reversal: confluent_reversal,
                    direction: None,
                })
            }
        }
    }

    pub fn find_confluent_pfs_direction(
        &self,
        pfs_cycles: &[Vec<PFS>],
        cycles: &[u32],
        ticker_data: &TickerData,
        target_date: &Time,
    ) -> Option<ConfluentPFSEvent> {
        match ticker_data
            .get_candles()
            .iter()
            .position(|c| &c.date == target_date)
        {
            None => {
                debug!(
                    "Failed to find index for confluent PFS direction target date: {}",
                    target_date.to_string()
                );
                return None;
            }
            Some(target_index) => {
                if target_index == 0 {
                    return None;
                }
                let prev_date = ticker_data
                    .get_candles()
                    .get(target_index - 1)
                    .expect("Failed to get previous candle")
                    .date;
                let target_date = ticker_data
                    .get_candles()
                    .get(target_index)
                    .expect("Failed to get current candle")
                    .date;

                let mut all_pfs_directions = Vec::new();
                let mut confluent_direction: Option<Direction> = None;
                for pfs in pfs_cycles.iter() {
                    let prev_pfs = pfs.iter().find(|p| p.date == prev_date);
                    let target_pfs = pfs.iter().find(|p| p.date == target_date);
                    if let (Some(prev_pfs), Some(target_pfs)) = (prev_pfs, target_pfs) {
                        if prev_pfs.value < target_pfs.value {
                            all_pfs_directions.push(Direction::Up);
                        } else if prev_pfs.value > target_pfs.value {
                            all_pfs_directions.push(Direction::Down);
                        } else {
                            debug!(
                                "Failed to find PFS direction for target date: {}",
                                target_date.to_string()
                            );
                        }
                    } else {
                        return None;
                    }
                }
                // determine if all PFS are up or down
                if all_pfs_directions.iter().all(|p| p == &Direction::Up) {
                    confluent_direction = Some(Direction::Up);
                } else if all_pfs_directions.iter().all(|p| p == &Direction::Down) {
                    confluent_direction = Some(Direction::Down);
                }

                Some(ConfluentPFSEvent {
                    date: target_date,
                    cycles: Some(cycles.to_vec()),
                    reversal: None,
                    direction: confluent_direction,
                })
            }
        }
    }

    /// Find the correlation for each individual PFS cycle
    pub fn individual_pfs_correlation(
        &self,
        ticker_data: &TickerData,
        pfs_cycles: Vec<Vec<PFS>>,
    ) -> Vec<IndividualPFSCorrelation> {
        let mut correlation = Vec::<IndividualPFSCorrelation>::new();

        for pfs in pfs_cycles {
            // iterate each date in time period
            // find previous candle and current candle and determine % change is position or negative
            let mut corr_count = 0;
            let mut total_count = 0;
            let time_period = self.start_date.time_period(&self.end_date);
            for (index, date) in time_period.iter().enumerate() {
                if index == 0 {
                    continue;
                }
                let prev_date = time_period
                    .get(index - 1)
                    .expect("Failed to get previous date");
                let prev_candle = ticker_data.candles.iter().find(|c| &c.date == prev_date);
                let current_candle = ticker_data.candles.iter().find(|c| &c.date == date);

                let mut candle_is_positive = None;
                let mut pfs_is_positive = None;

                // determine if % change is positive or negative
                if let (Some(prev_candle), Some(current_candle)) = (prev_candle, current_candle) {
                    let change = current_candle.percent_change(prev_candle.close);
                    if change > 0.0 {
                        candle_is_positive = Some(true);
                    } else {
                        candle_is_positive = Some(false);
                    }

                    // find PFS for current candle and previous candle and determine if PFS is positive or negative
                    let prev_pfs = pfs.iter().find(|p| &p.date == prev_date);
                    let pfs = pfs.iter().find(|p| &p.date == date);
                    if let (Some(prev_pfs), Some(pfs)) = (prev_pfs, pfs) {
                        if prev_pfs.value < pfs.value {
                            pfs_is_positive = Some(true);
                        } else {
                            pfs_is_positive = Some(false);
                        }
                    }
                }
                // if candle change and PFS change are the same, then increment positive correlation
                match (candle_is_positive, pfs_is_positive) {
                    (Some(true), Some(true)) => {
                        corr_count += 1;
                        total_count += 1;
                    }
                    (Some(false), Some(false)) => {
                        corr_count += 1;
                        total_count += 1;
                    }
                    (Some(true), Some(false)) => total_count += 1,
                    (Some(false), Some(true)) => total_count += 1,
                    _ => debug!(
                        "Failed to find candle or PFS for date: {}",
                        date.to_string_daily()
                    ),
                }
            }
            correlation.push(IndividualPFSCorrelation {
                cycle: pfs[0].cycle,
                hits: corr_count,
                total: total_count,
                pct_correlation: corr_count as f64 / total_count as f64,
            });
        }
        correlation
    }

    /// Find the correlation for each PFS cycle in confluence with price
    /// If all PFS cycles match the direction of price, then they are correlated
    fn confluent_pfs_direction_inner(
        start_date: Time,
        end_date: Time,
        ticker_data: &TickerData,
        pfs_cycles: Vec<Vec<PFS>>,
        cycles: &[u32],
    ) -> ConfluentPFSCorrelation {
        // iterate each date in time period
        // find previous candle and current candle and determine % change is position or negative
        let mut corr_count = 0;
        let mut total_count = 0;
        let mut events = Vec::<ConfluentPFSEvent>::new();

        let time_period = start_date.time_period(&end_date);
        for (index, date) in time_period.iter().enumerate() {
            if index == 0 {
                continue;
            }
            let prev_date = time_period
                .get(index - 1)
                .expect("Failed to get previous date");
            let prev_candle = ticker_data.candles.iter().find(|c| &c.date == prev_date);
            let current_candle = ticker_data.candles.iter().find(|c| &c.date == date);

            let mut candle_direction: Option<Direction> = None;
            let mut pfs_direction = Vec::<Option<Direction>>::new();
            let mut all_pfs_direction: Option<Direction> = None;

            // determine if % change is positive or negative
            if let (Some(prev_candle), Some(current_candle)) = (prev_candle, current_candle) {
                let change = current_candle.percent_change(prev_candle.close);
                if change > 0.0 {
                    candle_direction = Some(Direction::Up);
                } else {
                    candle_direction = Some(Direction::Down);
                }

                for pfs in pfs_cycles.iter() {
                    // find PFS for this cycle for current candle and previous candle and determine if PFS is positive or negative
                    let prev_pfs = pfs.iter().find(|p| &p.date == prev_date);
                    let curr_pfs = pfs.iter().find(|p| &p.date == date);
                    if let (Some(prev_pfs), Some(curr_pfs)) = (prev_pfs, curr_pfs) {
                        if prev_pfs.value < curr_pfs.value {
                            pfs_direction.push(Some(Direction::Up));
                        } else {
                            pfs_direction.push(Some(Direction::Down));
                        }
                    }
                }
                // determine if all PFS are positive or negative
                if pfs_direction.iter().all(|p| p == &Some(Direction::Up)) {
                    all_pfs_direction = Some(Direction::Up);
                } else if pfs_direction.iter().all(|p| p == &Some(Direction::Down)) {
                    all_pfs_direction = Some(Direction::Down);
                }
            }
            // if candle change and PFS change are the same, then increment positive correlation
            if candle_direction == Some(Direction::Up) && all_pfs_direction == Some(Direction::Up) {
                debug!("Candle positive && all PFS positive");
                events.push(ConfluentPFSEvent {
                    date: *date,
                    cycles: None,
                    direction: Some(Direction::Up),
                    reversal: None,
                });
                corr_count += 1;
                total_count += 1;
            } else if candle_direction == Some(Direction::Down)
                && all_pfs_direction == Some(Direction::Down)
            {
                debug!("Candle negative && all PFS negative");
                events.push(ConfluentPFSEvent {
                    date: *date,
                    cycles: None,
                    direction: Some(Direction::Down),
                    reversal: None,
                });
                corr_count += 1;
                total_count += 1;
            } else if candle_direction == Some(Direction::Up)
                && all_pfs_direction == Some(Direction::Down)
            {
                debug!("Candle positive && all PFS negative");
                total_count += 1;
            } else if candle_direction == Some(Direction::Down)
                && all_pfs_direction == Some(Direction::Up)
            {
                debug!("Candle negative && all PFS positive");
                total_count += 1;
            } else {
                debug!(
                    "Failed to find candle or PFS for date: {}",
                    date.to_string_daily()
                )
            }
        }
        ConfluentPFSCorrelation {
            cycles: cycles.to_vec(),
            events,
            hits: corr_count,
            total: total_count,
            pct_correlation: corr_count as f64 / total_count as f64,
        }
    }

    /// Find the correlation for each PFS cycle in confluence
    fn confluent_pfs_reversal_inner(
        start_date: Time,
        end_date: Time,
        pfs_cycles: Vec<Vec<PFS>>,
        cycles: &[u32],
    ) -> ConfluentPFSCorrelation {
        // iterate each date in time period
        // find previous candle and current candle and determine % change is position or negative
        let mut corr_count = 0;
        let mut total_count = 0;
        let mut events = Vec::<ConfluentPFSEvent>::new();

        let time_period = start_date.time_period(&end_date);
        for (index, date) in time_period.iter().enumerate() {
            if index == 0 || index == time_period.len() - 1 {
                continue;
            }
            let prev_date = time_period
                .get(index - 1)
                .expect("Failed to get previous date");
            let next_date = time_period.get(index + 1).expect("Failed to get next date");

            let mut pfs_is_reversal = Vec::<Option<ReversalType>>::new();
            let mut all_pfs_reversal_type: Option<ReversalType> = None;

            // determine if all PFS have a reversal on this date
            for pfs in pfs_cycles.iter() {
                let prev_pfs = pfs.iter().find(|p| &p.date == prev_date);
                let curr_pfs = pfs.iter().find(|p| &p.date == date);
                let next_pfs = pfs.iter().find(|p| &p.date == next_date);
                if let (Some(prev_pfs), Some(curr_pfs), Some(next_pfs)) =
                    (prev_pfs, curr_pfs, next_pfs)
                {
                    if prev_pfs.value < curr_pfs.value && curr_pfs.value > next_pfs.value {
                        println!("high, date: {}", date.to_string_daily());
                        pfs_is_reversal.push(Some(ReversalType::High));
                    } else if prev_pfs.value > curr_pfs.value && curr_pfs.value < next_pfs.value {
                        pfs_is_reversal.push(Some(ReversalType::Low));
                        println!("low, date: {}", date.to_string_daily());
                    }
                }
            }
            // determine if all PFS are highs or lows
            if pfs_is_reversal
                .iter()
                .all(|p| p == &Some(ReversalType::High))
            {
                all_pfs_reversal_type = Some(ReversalType::High);
            } else if pfs_is_reversal
                .iter()
                .all(|p| p == &Some(ReversalType::Low))
            {
                all_pfs_reversal_type = Some(ReversalType::Low);
            }

            if all_pfs_reversal_type == Some(ReversalType::High) {
                debug!("All PFS high");
                events.push(ConfluentPFSEvent {
                    date: *date,
                    cycles: Some(cycles.to_vec()),
                    direction: None,
                    reversal: Some(ReversalType::High),
                });
                corr_count += 1;
                total_count += 1;
            } else if all_pfs_reversal_type == Some(ReversalType::Low) {
                debug!("All PFS low");
                events.push(ConfluentPFSEvent {
                    date: *date,
                    cycles: Some(cycles.to_vec()),
                    direction: None,
                    reversal: Some(ReversalType::Low),
                });
                corr_count += 1;
                total_count += 1;
            } else if all_pfs_reversal_type.is_none() {
                debug!("All PFS neither high nor low");
                total_count += 1;
            } else {
                debug!(
                    "Failed to find confluent PFS for date: {}",
                    date.to_string_daily()
                );
                total_count += 1;
            }
        }
        ConfluentPFSCorrelation {
            cycles: cycles.to_vec(),
            events,
            hits: corr_count,
            total: total_count,
            pct_correlation: corr_count as f64 / total_count as f64,
        }
    }

    fn pfs_combinations(slice: &[u32], k: usize) -> Vec<Vec<u32>> {
        if k == 0 {
            return vec![Vec::new()];
        }
        if slice.len() < k {
            return Vec::new();
        }
        let mut result = Vec::new();
        let first = slice[0];
        let rest = &slice[1..];
        for mut combination in Self::pfs_combinations(rest, k - 1) {
            combination.insert(0, first);
            result.push(combination);
        }
        result.extend(Self::pfs_combinations(rest, k));
        result
    }

    pub fn pfs_cycles_for_timeframe(
        &self,
        ticker_data: &TickerData,
        cycles: &[u32],
        timeframe: PFSTimeframe,
    ) -> PFSResult<Vec<Vec<PFS>>> {
        let pfs_cycles = match timeframe {
            PFSTimeframe::Minute => {
                vec![]
            }
            PFSTimeframe::Hour => {
                vec![]
            }
            PFSTimeframe::Day => {
                let mut pfs_cycles = vec![];
                let mut threads = vec![];
                for cycle in cycles.iter() {
                    let ticker = ticker_data.clone();
                    let cycle = *cycle;
                    let start_date = self.start_date;
                    let end_date = self.end_date;
                    let thread = std::thread::spawn(move || {
                        Self::pfs_days(start_date, end_date, &ticker, cycle)
                    });
                    threads.push(thread);
                }
                for thread in threads {
                    let res: Vec<PFS> = thread.join().expect("Failed to join PFS cycle thread")?;
                    pfs_cycles.push(res);
                }
                pfs_cycles
            }
            PFSTimeframe::Month => {
                let mut pfs_cycles = vec![];
                let mut threads = vec![];
                for cycle in cycles.iter() {
                    let ticker = ticker_data.clone();
                    let cycle = *cycle;
                    let start_date = self.start_date;
                    let end_date = self.end_date;
                    let thread = std::thread::spawn(move || {
                        Self::pfs_months(start_date, end_date, &ticker, cycle)
                    });
                    threads.push(thread);
                }
                for thread in threads {
                    let res: Vec<PFS> = thread.join().expect("Failed to join PFS cycle thread");
                    pfs_cycles.push(res);
                }
                pfs_cycles
            }
            PFSTimeframe::Year => {
                let mut pfs_cycles = vec![];
                let mut threads = vec![];
                for cycle in cycles.iter() {
                    let ticker = ticker_data.clone();
                    let cycle = *cycle;
                    let start_date = self.start_date;
                    let end_date = self.end_date;
                    let thread = std::thread::spawn(move || {
                        Self::pfs_years(start_date, end_date, &ticker, cycle)
                    });
                    threads.push(thread);
                }
                for thread in threads {
                    let res: Vec<PFS> = thread.join().expect("Failed to join PFS cycle thread");
                    pfs_cycles.push(res);
                }
                pfs_cycles
            }
        };
        Ok(pfs_cycles)
    }

    pub fn confluent_pfs_direction(
        &self,
        ticker_data: &TickerData,
        cycles: &[u32],
        timeframe: PFSTimeframe,
        out_file: &str,
    ) -> PFSResult<Vec<ConfluentPFSCorrelation>> {
        let pfs_cycles = self.pfs_cycles_for_timeframe(ticker_data, cycles, timeframe)?;

        let mut correlations = Vec::<ConfluentPFSCorrelation>::new();
        for k in 1..=cycles.len() {
            let combs = Self::pfs_combinations(cycles, k);
            let mut comb_threads = vec![];
            for comb in combs.into_iter() {
                // find PFS for each cycle in combination
                let pfs_comb_cycles = comb
                    .iter()
                    .map(|c| {
                        let pfs = pfs_cycles.iter().find(|pfs| &pfs[0].cycle == c).unwrap();
                        pfs.to_vec()
                    })
                    .collect::<Vec<Vec<PFS>>>();
                let start_date = self.start_date;
                let end_date = self.end_date;
                let ticker_data = ticker_data.clone();
                let comb_thread = std::thread::spawn(move || {
                    Self::confluent_pfs_direction_inner(
                        start_date,
                        end_date,
                        &ticker_data,
                        pfs_comb_cycles,
                        &comb,
                    )
                });
                comb_threads.push(comb_thread);
            }
            for comb_thread in comb_threads {
                let res: ConfluentPFSCorrelation = comb_thread
                    .join()
                    .expect("Failed to join confluent PFS direction thread");
                correlations.push(res);
            }
        }
        // remove correlations that have no hits
        correlations.retain(|c| c.hits > 0);
        // sort correlations by highest correlation
        correlations.sort_by(|a, b| b.pct_correlation.partial_cmp(&a.pct_correlation).unwrap());
        self.write_pfs_confluence_csv(correlations.to_vec(), out_file)
            .expect("Failed to write PFS confluence CSV");
        Ok(correlations)
    }

    pub fn confluent_pfs_reversal(
        &self,
        ticker_data: &TickerData,
        cycles: &[u32],
        timeframe: PFSTimeframe,
        out_file: &str,
    ) -> PFSResult<Vec<ConfluentPFSCorrelation>> {
        let pfs_cycles = self.pfs_cycles_for_timeframe(ticker_data, cycles, timeframe)?;

        let mut correlations = Vec::<ConfluentPFSCorrelation>::new();
        for k in 1..=cycles.len() {
            let combs = Self::pfs_combinations(cycles, k);
            let mut threads = vec![];
            for comb in combs.into_iter() {
                // find PFS for each cycle in combination
                let pfs_cycles = comb
                    .iter()
                    .map(|c| {
                        let pfs = pfs_cycles.iter().find(|pfs| &pfs[0].cycle == c).unwrap();
                        pfs.to_vec()
                    })
                    .collect::<Vec<Vec<PFS>>>();
                let start_date = self.start_date;
                let end_date = self.end_date;
                let thread = std::thread::spawn(move || {
                    Self::confluent_pfs_reversal_inner(start_date, end_date, pfs_cycles, &comb)
                });
                threads.push(thread);
            }
            for thread in threads {
                let correlation = thread
                    .join()
                    .expect("Failed to join PFS confluent reversal thread");
                //(pfs_cycles, comb);
                correlations.push(correlation);
            }
        }
        // sort correlations by highest correlation
        if correlations.len() > 1 {
            correlations.sort_by(|a, b| b.pct_correlation.partial_cmp(&a.pct_correlation).unwrap());
        }
        // eliminate if correlation.cycles.len() == 1
        //let correlations = correlations.into_iter().filter(|c| c.cycles.len() > 1).collect::<Vec<ConfluentPFSCorrelation>>();
        self.write_pfs_confluence_csv(correlations.to_vec(), out_file)
            .expect("Failed to write PFS confluence CSV");
        Ok(correlations)
    }

    fn trade_quantity(&self, capital: f64, price: f64) -> f64 {
        let quantity = capital / price;
        (quantity * 1000000.0).round() / 1000000.0
    }

    fn stop_triggered(
        order: &Order,
        trailing_stop: &Option<f64>,
        stop_loss: &Option<f64>,
        candle: &Candle,
    ) -> bool {
        match order {
            Order::Long => {
                if trailing_stop.is_some() && stop_loss.is_some() {
                    candle.close < trailing_stop.unwrap() || candle.close < stop_loss.unwrap()
                } else if trailing_stop.is_some() && stop_loss.is_none() {
                    candle.close < trailing_stop.unwrap()
                } else if trailing_stop.is_none() && stop_loss.is_some() {
                    candle.close < stop_loss.unwrap()
                } else {
                    false
                }
            }
            Order::Short => {
                if trailing_stop.is_some() && stop_loss.is_some() {
                    candle.close > trailing_stop.unwrap() || candle.close > stop_loss.unwrap()
                } else if trailing_stop.is_some() && stop_loss.is_none() {
                    candle.close > trailing_stop.unwrap()
                } else if trailing_stop.is_none() && stop_loss.is_some() {
                    candle.close > stop_loss.unwrap()
                } else {
                    false
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn backtest_confluent_pfs_reversal(
        &self,
        ticker_data: &TickerData,
        cycles: &[u32],
        timeframe: PFSTimeframe,
        out_file: &str,
        capital: f64,
        trailing_stop_type: TrailingStopType,
        trailing_stop: f64,
        stop_loss_pct: f64,
    ) -> PFSResult<Vec<Backtest>> {
        let rev_corr = self.confluent_pfs_reversal(ticker_data, cycles, timeframe, out_file)?;

        let mut all_backtests = Vec::<Backtest>::new();
        for corr in rev_corr.iter() {
            // get PFS for each cycle in corr
            let cycles = corr.cycles.clone();
            let pfs_cycles = self.pfs_cycles_for_timeframe(ticker_data, &cycles, timeframe)?;

            let open_trade_mutex: Arc<Mutex<Option<Trade>>> = Arc::new(Mutex::new(None));
            let mut backtest = Backtest::new(capital);

            // isolate ticker candles from self.start_date to self.end_date
            let ticker_candles = ticker_data
                .get_candles()
                .iter()
                .filter(|c| c.date >= self.start_date && c.date <= self.end_date)
                .collect::<Vec<&Candle>>();

            for candle in ticker_candles {
                let date = &candle.date;
                // get PFS confluent reversal event for each date
                let event =
                    self.find_confluent_pfs_reversal(&pfs_cycles, &cycles, ticker_data, date);
                let mut open_trade = open_trade_mutex
                    .lock()
                    .expect("Failed to lock open_trade mutex");

                match event {
                    // if event, exit Long and enter Short if reversal is High
                    // if event, exit Short and enter Long if reversal is Low
                    Some(event) => {
                        if let Some(reversal) = event.reversal {
                            match reversal {
                                ReversalType::High => {
                                    debug!("PFS reversal High: {}", date.to_string_daily());
                                    // exit Long if price below trailing stop, or open trade is Long
                                    if let Some(trade) = &*open_trade {
                                        // clone is ok because value is overwritten after this block
                                        let mut trade = trade.clone();
                                        // let mut trade = trade.clone();
                                        if trade.order == Order::Long
                                            || Self::stop_triggered(
                                                &trade.order,
                                                &trade.trailing_stop,
                                                &trade.stop_loss,
                                                candle,
                                            )
                                        {
                                            trade.exit(*date, candle.close);
                                            backtest.add_trade(trade);
                                            *open_trade = None;
                                        }
                                    }
                                    // enter short
                                    let qty = self.trade_quantity(capital, candle.close);
                                    let trailing_stop = Trade::calc_trailing_stop(
                                        Order::Short,
                                        candle.close,
                                        trailing_stop_type,
                                        trailing_stop,
                                    );
                                    let stop_loss = Trade::calc_stop_loss(
                                        Order::Short,
                                        candle.close,
                                        stop_loss_pct,
                                    );
                                    *open_trade = Some(Trade::new(
                                        *date,
                                        Order::Short,
                                        qty,
                                        candle.close,
                                        capital,
                                        Some(trailing_stop),
                                        Some(stop_loss),
                                    ));
                                }
                                ReversalType::Low => {
                                    debug!("PFS Low: {}", date.to_string_daily());
                                    // exit short
                                    if let Some(trade) = &*open_trade {
                                        // clone is ok because value is overwritten after this block
                                        let mut trade = trade.clone();
                                        if trade.order == Order::Short
                                            || Self::stop_triggered(
                                                &trade.order,
                                                &trade.trailing_stop,
                                                &trade.stop_loss,
                                                candle,
                                            )
                                        {
                                            trade.exit(*date, candle.close);
                                            backtest.add_trade(trade);
                                            *open_trade = None;
                                        }
                                    }
                                    // enter long
                                    let qty = Trade::trade_quantity(capital, candle.close);
                                    let trailing_stop = Trade::calc_trailing_stop(
                                        Order::Long,
                                        candle.close,
                                        trailing_stop_type,
                                        trailing_stop,
                                    );
                                    let stop_loss = Trade::calc_stop_loss(
                                        Order::Long,
                                        candle.close,
                                        stop_loss_pct,
                                    );
                                    *open_trade = Some(Trade::new(
                                        *date,
                                        Order::Long,
                                        qty,
                                        candle.close,
                                        capital,
                                        Some(trailing_stop),
                                        Some(stop_loss),
                                    ));
                                }
                            }
                        }
                    }
                    // if no event, check trailing stop
                    // if trailing stop is hit, exit trade
                    // otherwise update trailing stop
                    None => {
                        debug!("No PFS Reversal: {}", date.to_string_daily());
                        if let Some(trade) = &*open_trade {
                            match trade.order {
                                Order::Long => {
                                    // Long trailing stop is hit, exit trade
                                    if Self::stop_triggered(
                                        &trade.order,
                                        &trade.trailing_stop,
                                        &trade.stop_loss,
                                        candle,
                                    ) {
                                        let mut trade = trade.clone();
                                        trade.exit(*date, candle.close);
                                        backtest.add_trade(trade);
                                    }
                                    // Long trailing stop is not hit, update trailing stop
                                    else {
                                        let mut trade = trade.clone();
                                        trade.trailing_stop = Some(Trade::calc_trailing_stop(
                                            Order::Long,
                                            candle.close,
                                            trailing_stop_type,
                                            trailing_stop,
                                        ));
                                        *open_trade = Some(trade);
                                    }
                                }
                                Order::Short => {
                                    // Short trailing stop is hit, exit trade
                                    if Self::stop_triggered(
                                        &trade.order,
                                        &trade.trailing_stop,
                                        &trade.stop_loss,
                                        candle,
                                    ) {
                                        let mut trade = trade.clone();
                                        trade.exit(*date, candle.close);
                                        backtest.add_trade(trade);
                                    }
                                    // Short trailing stop is not hit, update trailing stop
                                    else {
                                        let mut trade = trade.clone();
                                        trade.trailing_stop = Some(Trade::calc_trailing_stop(
                                            Order::Short,
                                            candle.close,
                                            trailing_stop_type,
                                            trailing_stop,
                                        ));
                                        *open_trade = Some(trade);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            backtest.summarize();
            all_backtests.push(backtest);
        }
        all_backtests.sort_by(|a, b| b.pnl.partial_cmp(&a.pnl).unwrap());
        Self::write_pfs_confluence_backtest_csv(all_backtests.to_vec(), out_file)
            .expect("Failed to write PFS confluence backtest to CSV");
        Ok(all_backtests)
    }

    fn write_pfs_confluence_backtest_csv(
        backtests: Vec<Backtest>,
        out_file: &str,
    ) -> Result<(), Box<dyn Error>> {
        if backtests.is_empty() {
            return Err("No backtests found".into());
        }
        let mut file = File::create(out_file)?;

        writeln!(
            file,
            "start_date,end_date,pnl,avg_trade,avg_win,avg_loss,win_trades,loss_trades,trades"
        )?;
        for backtest in backtests.iter() {
            if backtest.trades.is_empty() {
                continue;
            }
            let start_date = backtest
                .start_date
                .expect("No start date found")
                .to_string_daily();
            let end_date = backtest
                .end_date
                .expect("No end date found")
                .to_string_daily();
            let pnl = backtest.pnl.unwrap_or(0.0);
            let avg_trade = backtest.avg_trade_pnl.unwrap_or(0.0);
            let avg_win = backtest.avg_win_trade_pnl.unwrap_or(0.0);
            let avg_loss = backtest.avg_loss_trade_pnl.unwrap_or(0.0);
            let win_trades = backtest.num_win_trades();
            let loss_trades = backtest.num_loss_trades();
            let trades = backtest.trades.len();
            writeln!(
                file,
                "{},{},{},{},{},{},{},{},{}",
                start_date,
                end_date,
                pnl,
                avg_trade,
                avg_win,
                avg_loss,
                win_trades,
                loss_trades,
                trades
            )?;
        }
        Ok(())
    }

    fn write_pfs_confluence_csv(
        &self,
        correlations: Vec<ConfluentPFSCorrelation>,
        out_file: &str,
    ) -> Result<(), Box<dyn Error>> {
        if correlations.is_empty() {
            return Err("No correlations found".into());
        }
        let mut file = File::create(out_file)?;

        writeln!(file, "start_date,end_date,correlation,hits,total,cycles")?;
        // format Vec<u32> into format that implements Display
        for corr in correlations.iter() {
            let cycles = corr
                .cycles
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<String>>()
                .join(",");
            writeln!(
                file,
                "{},{},{},{},{},[{}]",
                self.start_date.to_string_daily(),
                self.end_date.to_string_daily(),
                corr.pct_correlation,
                corr.hits,
                corr.total,
                cycles
            )?;
        }
        Ok(())
    }

    pub fn plot_pfs(
        &self,
        daily_pfs: &[PFS],
        out_file: &str,
        plot_title: &str,
        plot_color: &RGBColor,
    ) {
        // get daily PFS data
        let data = self.get_data(daily_pfs);
        // draw chart
        let root = BitMapBackend::new(out_file, (2048, 1024)).into_drawing_area();
        root.fill(&WHITE).unwrap();
        // PFS start date
        let from_date_index = self.find_date_index(&data, &self.start_date);
        let from_date_input = self.parse_time(&data[from_date_index].0);
        let from_date = from_date_input - Duration::days(1);
        println!("PFS Start Date: {}", from_date);
        // PFS end date
        let to_date_index = self.find_date_index(&data, &self.end_date);
        let to_date_input = self.parse_time(&data[to_date_index].0);
        let to_date = to_date_input + Duration::days(1);
        println!("PFS End Date: {}", to_date);
        // label chart
        let y_min = daily_pfs[from_date_index..to_date_index]
            .iter()
            .map(|x| x.value)
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap() as f32;
        let y_max = daily_pfs[from_date_index..to_date_index]
            .iter()
            .map(|x| x.value)
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap() as f32;
        let mut chart = ChartBuilder::on(&root)
            .x_label_area_size(40)
            .y_label_area_size(40)
            .caption(plot_title, ("sans-serif", 50.0).into_font())
            .build_cartesian_2d(from_date..to_date, y_min..y_max)
            .unwrap();
        chart
            .configure_mesh()
            .light_line_style(WHITE)
            .draw()
            .unwrap();
        // plot PFS values
        chart
            .draw_series(
                LineSeries::new(
                    data.iter().map(|x| (self.parse_time(&x.0), x.1)),
                    ShapeStyle {
                        color: RGBAColor::from(*plot_color),
                        filled: true,
                        stroke_width: 2,
                    },
                )
                .point_size(5),
            )
            .unwrap();
        // To avoid the IO failure being ignored silently, we manually call the present function
        root.present().expect("Unable to write result to file, please make sure 'plotters-doc-data' dir exists under current dir");
        println!("Result has been saved to {}", out_file);
    }

    fn get_data(&self, daily_pfs: &[PFS]) -> Vec<(String, f32)> {
        let mut data = Vec::new();
        for pfs in daily_pfs.iter() {
            data.push((pfs.date.to_string_daily(), pfs.value as f32));
        }
        data
    }

    fn find_date_index(&self, data: &[(String, f32)], date: &Time) -> usize {
        for (i, (d, _)) in data.iter().enumerate() {
            if d == &date.to_string_daily() {
                return i;
            }
        }
        let mut change_date = *date;
        info!(
            "Entering infinite loop to find previous date index for date: {}",
            date.to_string_daily()
        );
        loop {
            change_date = change_date.delta_date(-1);
            // get previous index in data
            for (i, (d, _)) in data.iter().enumerate() {
                if d == &change_date.to_string_daily() {
                    return i;
                }
            }
        }
    }

    fn parse_time(&self, t: &str) -> NaiveDate {
        Local
            .datetime_from_str(&format!("{} 0:0", t), "%Y-%m-%d %H:%M")
            .unwrap_or_else(|_| panic!("Failed to parse time {}", t))
            .date_naive()
    }
}
