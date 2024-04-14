use crate::*;
use chrono::{Local, NaiveDate, TimeZone};
use log::trace;
use plotters::prelude::*;

/// Historical Date Analysis
pub struct HDA {
    /// Reversal on this date
    pub date: Time,
    /// How many years had a reversal on this date
    pub mode: u32,
}

impl HDA {
    pub fn new(date: Time, mode: u32) -> Self {
        Self { date, mode }
    }
}

pub struct PlotHDA {
    /// Start date to plot daily HDA
    pub start_date: Time,
    /// End date to plot daily HDA
    pub end_date: Time,
    /// Candle reversal higher/lower than these bars to the left
    pub left_bars: usize,
    /// Candle reversal higher/lower than these bars to the right
    pub right_bars: usize,
    /// Candle is within this margin of a reversal (valid HDA)
    pub hda_margin: usize,
}

impl PlotHDA {
    pub fn new(
        start_date: Time,
        end_date: Time,
        left_bars: usize,
        right_bars: usize,
        hda_margin: usize,
    ) -> Self {
        Self {
            start_date,
            end_date,
            left_bars,
            right_bars,
            hda_margin,
        }
    }

    fn highs_past_period(&self, ticker_data: &TickerData) -> Vec<Time> {
        let highs = ticker_data.pivot_highs(self.left_bars, self.right_bars);
        let time_period = self.start_date.time_period(&self.end_date);
        let mut highs_in_period = Vec::<Time>::new();
        for high in highs.iter() {
            if time_period.contains(&high.date) {
                highs_in_period.push(high.date);
            }
        }
        highs_in_period
    }

    fn lows_past_period(&self, ticker_data: &TickerData) -> Vec<Time> {
        let lows = ticker_data.pivot_lows(self.left_bars, self.right_bars);
        let time_period = self.start_date.time_period(&self.end_date);
        let mut lows_in_period = Vec::<Time>::new();
        for high in lows.iter() {
            if time_period.contains(&high.date) {
                lows_in_period.push(high.date);
            }
        }
        lows_in_period
    }

    /// Compute Historical Date Analysis
    /// Compares the same date of each year for similar price action.
    /// Find reversals in the specified time period.
    // Iterate each year back and search for reversals on +/- 1 day from those reversals
    // If a reversal is found on +/- 1 day from the first reversal, add year to HDA `filter_years`
    // Iterate `filter_years` and find years that have a reversal on the second reversal in sequence from the first.
    // Update `filter_years` with years that match this second reversal
    // Push the mode of each reversal date to `daily_hda`
    pub fn hda(&self, ticker_data: &TickerData) -> Vec<HDA> {
        let mut daily_hda = Vec::<HDA>::new();

        // compute number of cycles possible in candle history
        let earliest_date = ticker_data.earliest_date();
        let earliest_candle_year = earliest_date.year;
        let latest_date = ticker_data.latest_date();
        let latest_candle_year = latest_date.year;
        let total_years_back = latest_candle_year - earliest_candle_year + 1;

        let mut filter_years = Vec::<i32>::new();
        let highs_in_period = self.highs_past_period(ticker_data);
        let lows_in_period = self.lows_past_period(ticker_data);
        let reversals = highs_in_period
            .iter()
            .chain(lows_in_period.iter())
            .collect::<Vec<&Time>>();

        // find years that match 2 reversal dates in the time period within a margin of error (hda margin)
        for reversal in reversals.iter() {
            let mut matching_reversals = 0;
            for years_back in 1..total_years_back {
                for (index, candle) in ticker_data.get_candles().iter().enumerate() {
                    if index == 0 {
                        continue;
                    }
                    let prev_candle = ticker_data
                        .get_candles()
                        .get(index - 1)
                        .expect("Failed to get previous candle");
                    let cycle_date = Time::new(
                        reversal.year - years_back,
                        &reversal.month,
                        &reversal.day,
                        None,
                        None,
                    );
                    if (prev_candle.date < cycle_date && candle.date >= cycle_date)
                        && ticker_data.candle_is_reversal(
                            candle,
                            self.left_bars,
                            self.right_bars,
                            self.hda_margin,
                        )
                    {
                        matching_reversals += 1;
                    }
                }
                if matching_reversals > 1 {
                    filter_years.push(reversal.year - years_back);
                }
            }
        }
        self.remove_duplicate_years(&mut filter_years);

        // iterate start date to end date and find reversals in past `filter_years` that match reversal date in the time period
        let time_period = self.start_date.time_period(&self.end_date);
        for date in time_period.iter() {
            // HDA for this date (frequency of reversals on this date across all years)
            let mut hda = 0;
            // find candle in `filter_year`
            for (index, candle) in ticker_data.get_candles().iter().enumerate() {
                if index == 0 {
                    continue;
                }
                for filter_year in filter_years.iter() {
                    // candle date X years back
                    let cycle_date = Time::new(*filter_year, &date.month, &date.day, None, None);
                    // found candle in previous year on this date
                    let prev_candle = ticker_data
                        .candles
                        .get(index - 1)
                        .expect("Failed to get previous candle");
                    if prev_candle.date < cycle_date && candle.date >= cycle_date {
                        // if candle is within margin of local high or low
                        if ticker_data.candle_is_reversal(
                            candle,
                            self.left_bars,
                            self.right_bars,
                            self.hda_margin,
                        ) {
                            hda += 1;
                        }
                    }
                }
            }
            daily_hda.push(HDA {
                date: *date,
                mode: hda,
            });
        }
        daily_hda
    }

    fn remove_duplicate_years(&self, years: &mut Vec<i32>) {
        years.sort();
        years.dedup();
    }

    pub fn plot_hda(
        &self,
        daily_hda: &[HDA],
        out_file: &str,
        plot_title: &str,
        plot_color: &RGBColor,
    ) {
        if self.start_date > self.end_date {
            println!(
                "Start date {} is after end date {}",
                self.start_date.to_string(),
                self.end_date.to_string()
            );
            return;
        }
        if daily_hda.is_empty() {
            println!("No HDA data to plot");
            return;
        }
        if self.start_date < daily_hda[0].date {
            println!(
                "Start date {} is before earliest HDA date {}",
                self.start_date.to_string(),
                daily_hda[0].date.to_string()
            );
            return;
        } else if self.end_date > daily_hda[daily_hda.len() - 1].date {
            println!(
                "End date {} is after latest HDA date {}",
                self.end_date.to_string(),
                daily_hda[daily_hda.len() - 1].date.to_string()
            );
            return;
        }
        // get daily PFS data
        let data = self.get_data(daily_hda);
        // draw chart
        let root = BitMapBackend::new(out_file, (2048, 1024)).into_drawing_area();
        root.fill(&WHITE).unwrap();
        // PFS start date
        let from_date_index = self.find_date_index(&data, &self.start_date);
        let from_date = self.parse_time(&data[from_date_index].0);
        println!("HDA Start Date: {}", from_date);
        // PFS end date
        let to_date_index = self.find_date_index(&data, &self.end_date);
        let to_date = self.parse_time(&data[to_date_index].0);
        println!("HDA End Date: {}", to_date);

        // find minimum value in data
        let min = data.iter().fold(100f32, |min, x| min.min(x.1));
        let max = data.iter().fold(0f32, |max, x| max.max(x.1));

        // label chart
        let mut chart = ChartBuilder::on(&root)
            .x_label_area_size(40)
            .y_label_area_size(40)
            .caption(plot_title, ("sans-serif", 50.0).into_font())
            .build_cartesian_2d(from_date..to_date, min..max)
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

    fn get_data(&self, daily_hda: &[HDA]) -> Vec<(String, f32)> {
        let mut data = Vec::new();
        for hda in daily_hda.iter() {
            data.push((hda.date.to_string_daily(), hda.mode as f32));
        }
        data
    }

    fn find_date_index(&self, data: &[(String, f32)], date: &Time) -> usize {
        for (i, (d, _)) in data.iter().enumerate() {
            if d == &date.to_string_daily() {
                return i;
            }
        }
        let mut _not_found = true;
        let mut change_date = *date;
        while _not_found {
            change_date = change_date.delta_date(-1);
            // get previous index in data
            for (i, (d, _)) in data.iter().enumerate() {
                if d == &change_date.to_string_daily() {
                    _not_found = false;
                    return i;
                }
            }
        }
        trace!("not_found: {}", _not_found);
        panic!("Date not found");
    }

    fn parse_time(&self, t: &str) -> NaiveDate {
        Local
            .datetime_from_str(&format!("{} 0:0", t), "%Y-%m-%d %H:%M")
            .unwrap_or_else(|_| panic!("Failed to parse time {}", t))
            .date_naive()
    }
}
