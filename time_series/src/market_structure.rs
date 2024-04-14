use crate::backtest::Direction;
use crate::{Candle, Reversal, ReversalType, TickerData};
use log::debug;
use std::path::PathBuf;

#[derive(Debug)]
pub enum MarketStructureError {
    NoFirstLow,
    NoFirstHigh,
}

impl std::fmt::Display for MarketStructureError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            MarketStructureError::NoFirstLow => write!(f, "No major low found"),
            MarketStructureError::NoFirstHigh => write!(f, "No major high found"),
        }
    }
}

pub type MarketStructureResult<T> = Result<T, MarketStructureError>;

#[derive(Debug, Clone)]
pub struct Trend {
    pub start_candle: Option<Candle>,
    pub end_candle: Option<Candle>,
    pub reversal: Option<Reversal>,
    pub direction: Option<Direction>,
}

#[derive(Clone, Debug)]
pub struct MarketStructure {
    pub candles: Vec<Candle>,
    pub reversals: Vec<Reversal>,
    pub trends: Vec<Trend>,
    pub latest_high: Option<Candle>,
    pub latest_low: Option<Candle>,
    pub reversal_candle_range: usize,
}

impl MarketStructure {
    /// Identify market structure in vector of reversals .
    /// by finding higher highs and higher lows for positive market structure,
    /// and lower highs and lower lows for negative market structure.
    pub fn new(ticker_data: &TickerData, candle_range: usize) -> Self {
        let mut trends = Vec::<Trend>::new();
        let reversals = ticker_data.find_reversals(candle_range);
        debug!(
            "First Candle: {:?}",
            ticker_data.candles[0].date.to_string()
        );
        debug!(
            "Last Candle: {:?}",
            ticker_data.candles[ticker_data.candles.len() - 1]
                .date
                .to_string()
        );
        debug!("First Reversal: {:?}", reversals[0].candle.date.to_string());
        debug!(
            "Last Reversal: {:?}",
            reversals[reversals.len() - 1].candle.date.to_string()
        );

        let mut direction: Option<Direction> = None;
        let mut start_candle: Option<Candle> = None;
        let mut latest_low: Option<Candle> = None;
        let mut latest_high: Option<Candle> = None;
        // iterate lows and identify series of higher lows
        for reversal in reversals.iter() {
            match direction {
                // no trend established yet
                None => {
                    start_candle = Some(reversal.candle.clone());
                    match reversal.reversal_type {
                        ReversalType::High => {
                            if let Some(latest_high) = &latest_high {
                                // positive trend
                                if reversal.candle.high > latest_high.high {
                                    trends.push(Trend {
                                        start_candle: start_candle.clone(),
                                        end_candle: None,
                                        reversal: Some(reversal.clone()),
                                        direction: Some(Direction::Up),
                                    });
                                    direction = Some(Direction::Up);
                                }
                                // negative trend
                                else {
                                    trends.push(Trend {
                                        start_candle: start_candle.clone(),
                                        end_candle: None,
                                        reversal: Some(reversal.clone()),
                                        direction: Some(Direction::Down),
                                    });
                                    direction = Some(Direction::Down);
                                }
                            }
                            latest_high = Some(reversal.candle.clone());
                        }
                        ReversalType::Low => {
                            if let Some(latest_low) = &latest_low {
                                // positive trend
                                if reversal.candle.low > latest_low.low {
                                    trends.push(Trend {
                                        start_candle: start_candle.clone(),
                                        end_candle: None,
                                        reversal: Some(reversal.clone()),
                                        direction: Some(Direction::Up),
                                    });
                                    direction = Some(Direction::Up);
                                }
                                // negative trend
                                else {
                                    trends.push(Trend {
                                        start_candle: start_candle.clone(),
                                        end_candle: None,
                                        reversal: Some(reversal.clone()),
                                        direction: Some(Direction::Down),
                                    });
                                    direction = Some(Direction::Down);
                                }
                            }
                            latest_low = Some(reversal.candle.clone());
                        }
                    }
                }
                // positive market structure
                Some(Direction::Up) => {
                    match reversal.reversal_type {
                        // compare current high to previous high
                        ReversalType::High => {
                            if let Some(latest_high) = &latest_high {
                                // positive trend continues
                                if reversal.candle.high > latest_high.high {
                                    trends.push(Trend {
                                        start_candle: start_candle.clone(),
                                        end_candle: None,
                                        reversal: Some(reversal.clone()),
                                        direction: Some(Direction::Up),
                                    });
                                }
                                // positive trend ends
                                else {
                                    trends.push(Trend {
                                        start_candle: start_candle.clone(),
                                        end_candle: Some(reversal.candle.clone()),
                                        reversal: Some(reversal.clone()),
                                        direction: None,
                                    });
                                    direction = None;
                                }
                            }
                            latest_high = Some(reversal.candle.clone());
                        }
                        // compare current low to previous low
                        ReversalType::Low => {
                            if let Some(latest_low) = &latest_low {
                                // positive trend continues
                                if reversal.candle.low > latest_low.low {
                                    trends.push(Trend {
                                        start_candle: start_candle.clone(),
                                        end_candle: None,
                                        reversal: Some(reversal.clone()),
                                        direction: Some(Direction::Up),
                                    });
                                }
                                // positive trend ends
                                else {
                                    trends.push(Trend {
                                        start_candle: start_candle.clone(),
                                        end_candle: Some(reversal.candle.clone()),
                                        reversal: Some(reversal.clone()),
                                        direction: None,
                                    });
                                    direction = None;
                                }
                            }
                            latest_low = Some(reversal.candle.clone());
                        }
                    }
                }
                // negative market structure
                Some(Direction::Down) => {
                    match reversal.reversal_type {
                        // compare current high to previous high
                        ReversalType::High => {
                            if let Some(latest_high) = &latest_high {
                                // negative trend continues
                                if reversal.candle.high < latest_high.high {
                                    trends.push(Trend {
                                        start_candle: start_candle.clone(),
                                        end_candle: None,
                                        reversal: Some(reversal.clone()),
                                        direction: Some(Direction::Down),
                                    });
                                }
                                // negative trend ends
                                else {
                                    trends.push(Trend {
                                        start_candle: start_candle.clone(),
                                        end_candle: Some(reversal.candle.clone()),
                                        reversal: Some(reversal.clone()),
                                        direction: None,
                                    });
                                    direction = None;
                                }
                            }
                            latest_high = Some(reversal.candle.clone());
                        }
                        // compare current low to previous low
                        ReversalType::Low => {
                            if let Some(latest_low) = &latest_low {
                                // negative trend continues
                                if reversal.candle.low < latest_low.low {
                                    trends.push(Trend {
                                        start_candle: start_candle.clone(),
                                        end_candle: None,
                                        reversal: Some(reversal.clone()),
                                        direction: Some(Direction::Down),
                                    });
                                }
                                // negative trend ends
                                else {
                                    trends.push(Trend {
                                        start_candle: start_candle.clone(),
                                        end_candle: Some(reversal.candle.clone()),
                                        reversal: Some(reversal.clone()),
                                        direction: None,
                                    });
                                    direction = None;
                                }
                            }
                            latest_low = Some(reversal.candle.clone());
                        }
                    }
                }
            }
        }

        Self {
            candles: ticker_data.candles.clone(),
            reversals,
            trends,
            latest_high,
            latest_low,
            reversal_candle_range: candle_range,
        }
    }

    pub fn first_low(&self) -> MarketStructureResult<Reversal> {
        let first_low = self
            .reversals
            .iter()
            .find(|r| r.reversal_type == ReversalType::Low);
        match first_low {
            Some(low) => Ok(low.clone()),
            None => Err(MarketStructureError::NoFirstLow),
        }
    }

    pub fn first_high(&self) -> MarketStructureResult<Reversal> {
        let first_high = self
            .reversals
            .iter()
            .find(|r| r.reversal_type == ReversalType::High);
        match first_high {
            Some(high) => Ok(high.clone()),
            None => Err(MarketStructureError::NoFirstHigh),
        }
    }

    pub fn test_market_structure(candle_range: usize, results_file: &PathBuf) {
        let mut ticker_data = TickerData::new();
        ticker_data
            .add_csv_series(results_file)
            .expect("Failed to create TickerData");
        let market_structure = MarketStructure::new(&ticker_data, candle_range);

        match &market_structure.latest_high {
            Some(high) => println!("Latest High: {}", high.date.to_string()),
            None => println!("Latest High: None"),
        };
        match &market_structure.latest_low {
            Some(low) => println!("Latest Low: {}", low.date.to_string()),
            None => println!("Latest Low: None"),
        };

        println!("START\t\tEND\t\tREVERSAL\t\tTREND");
        for trend in market_structure.trends.iter() {
            match &trend.start_candle {
                Some(candle) => print!("{}", candle.date.to_string()),
                None => print!("None"),
            };
            match &trend.end_candle {
                Some(candle) => print!("\t{}", candle.date.to_string()),
                None => print!("\tNone\t"),
            };
            match &trend.reversal {
                Some(reversal) => print!("\t{}\t\t", reversal.candle.date.to_string()),
                None => print!("\tNone\t\t"),
            };
            print!("{:?}", trend.direction.as_ref());
            println!();
        }
    }
}
