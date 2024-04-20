use crate::{Candle, Data, Kagi, KagiDirection, RollingCandles, Signal, Source, Time, trunc};
use std::fs::File;
use std::path::PathBuf;
use std::str::FromStr;
use log::LevelFilter;
use serde::{Serialize, Deserialize};
use crate::dreamrunner::Dreamrunner;


#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Order {
    Long,
    Short,
}

#[derive(Debug, Clone)]
pub struct Trade {
    pub date: Time,
    pub side: Order,
    /// base asset quantity
    pub quantity: f64,
    pub price: f64,
    pub capital: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    start_date: String,
    end_date: String,
    avg_trade_quote_pnl: f64,
    num_winners: usize,
    num_losers: usize,
    win_pct: f64
}

#[derive(Debug, Clone, Default)]
pub struct Backtest {
    pub candles: Vec<Candle>,
    pub trades: Vec<Trade>
}
impl Backtest {
    pub fn new() -> Self {
        Self::default()
    }

    /// Read candles from CSV file.
    /// Handles duplicate candles and sorts candles by date.
    /// Expects date of candle to be in UNIX timestamp format.
    /// CSV format: date,open,high,low,close,volume
    pub fn add_csv_series(&mut self, csv_path: &PathBuf, start_time: Option<Time>, end_time: Option<Time>) -> anyhow::Result<()> {
        let file_buffer = File::open(csv_path)?;
        let mut csv = csv::Reader::from_reader(file_buffer);

        let mut headers = Vec::new();
        if let Ok(result) = csv.headers() {
            for header in result {
                headers.push(String::from(header));
            }
        }
        
        for record in csv.records().flatten() {
            let date = Time::from_unix(
                record[0]
                  .parse::<i64>()
                  .expect("failed to parse candle UNIX timestamp into i64"),
            );
            let volume = None;
            let candle = Candle {
                date,
                open: f64::from_str(&record[1]).expect("failed to parse open"),
                high: f64::from_str(&record[2]).expect("failed to parse high"),
                low: f64::from_str(&record[3]).expect("failed to parse low"),
                close: f64::from_str(&record[4]).expect("failed to parse close"),
                volume,
            };
            self.add_candle(candle);
        }
        // only take candles greater than a timestamp
        self.candles.retain(|candle| {
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
        
        Ok(())
    }

    pub fn add_candle(&mut self, candle: Candle) {
        self.candles.push(candle);
    }

    pub fn add_trade(&mut self, trade: Trade) {
        self.trades.push(trade);
    }

    pub fn cum_quote_pnl_history(&self) -> anyhow::Result<Vec<Data>> {
        let trades = &self.trades;
        let mut cum = 0.0;
        let mut data = Vec::new();
        for trades in trades.windows(2) {
            let exit = &trades[1];
            let entry = &trades[0];
            let factor = match entry.side {
                Order::Long => 1.0,
                Order::Short => -1.0,
            };
            let deci_pnl = (exit.price - entry.price) / entry.price * factor;
            let quote_pnl = deci_pnl * entry.quantity * entry.price;
            cum += quote_pnl;
            data.push(Data {
                x: entry.date.to_unix_ms(),
                y: cum
            });
        }
        Ok(data)
    }

    pub fn avg_trade_quote_pnl(&self) -> anyhow::Result<f64> {
        let trades = &self.trades;
        let mut pnls = Vec::new();
        for trades in trades.windows(2) {
            // latest trade is last
            let exit = &trades[1];
            let entry = &trades[0];
            let factor = match entry.side {
                Order::Long => 1.0,
                Order::Short => -1.0,
            };
            let deci_pnl = (exit.price - entry.price) / entry.price * factor;
            let quote_pnl = deci_pnl * entry.quantity * entry.price;
            pnls.push(quote_pnl);
        }
        let avg = pnls.iter().sum::<f64>() / pnls.len() as f64;
        Ok(avg)
    }

    pub fn quote_pnl(&self) -> anyhow::Result<f64> {
        let trades = &self.trades;
        let mut cum_pnl = 0.0;
        for trades in trades.windows(2) {
            let exit = &trades[1];
            let entry = &trades[0];
            let factor = match entry.side {
                Order::Long => 1.0,
                Order::Short => -1.0,
            };
            let deci_pnl = (exit.price - entry.price) / entry.price * factor;
            let quote_pnl = deci_pnl * entry.quantity * entry.price;
            cum_pnl += quote_pnl;
        }
        Ok(trunc!(cum_pnl, 4))
    }

    pub fn num_trades(&self) -> usize {
        self.trades.len()
    }

    pub fn num_winners(&self) -> usize {
        let mut wins = 0;
        let trades = &self.trades;
        for trades in trades.windows(2) {
            let exit = &trades[1];
            let entry = &trades[0];
            let factor = match entry.side {
                Order::Long => 1.0,
                Order::Short => -1.0,
            };
            let deci_pnl = (exit.price - entry.price) / entry.price * factor;
            let quote_pnl = deci_pnl * entry.quantity * entry.price;
            if quote_pnl > 0.0 {
                wins += 1;
            }
        }
        wins
    }

    pub fn num_losers(&self) -> usize {
        let mut loses = 0;
        let trades = &self.trades;
        for trades in trades.windows(2) {
            let exit = &trades[1];
            let entry = &trades[0];
            let factor = match entry.side {
                Order::Long => 1.0,
                Order::Short => -1.0,
            };
            let deci_pnl = (exit.price - entry.price) / entry.price * factor;
            let quote_pnl = deci_pnl * entry.quantity * entry.price;
            if quote_pnl < 0.0 {
                loses += 1;
            }
        }
        loses
    }

    pub fn summarize(&mut self) -> anyhow::Result<Summary> {
        Ok(Summary {
            start_date: self.candles.first().unwrap().date.to_string(),
            end_date: self.candles.last().unwrap().date.to_string(),
            avg_trade_quote_pnl: self.avg_trade_quote_pnl()?,
            num_winners: self.num_winners(),
            num_losers: self.num_losers(),
            win_pct: self.num_winners() as f64 / self.num_trades() as f64 * 100.0,
        })
    }

    pub fn wmas(
        &mut self,
        wma_period: usize,
        k_rev: f64,
        k_src: Source,
        ma_src: Source
    ) -> anyhow::Result<Vec<Data>> {
        let mut period = RollingCandles::new(wma_period + 1);
        let dreamrunner = Dreamrunner {
            k_rev,
            k_src,
            ma_src
        };
        let mut data = Vec::new();

        let candles = self.candles.clone();
        for candle in candles {
            period.push(candle);
            let period_from_curr: Vec<&Candle> = period.vec.range(0..period.vec.len() - 1).collect();
            data.push(Data {
                x: candle.date.to_unix_ms(),
                y: dreamrunner.wma(&period_from_curr)
            });
        }
        Ok(data)
    }

    pub fn kagis(
        &mut self,
        wma_period: usize,
        k_rev: f64,
        k_src: Source,
        ma_src: Source
    ) -> anyhow::Result<Vec<Data>> {
        let mut period = RollingCandles::new(wma_period + 1);
        let mut kagi = Kagi {
            line: self.candles.first().unwrap().low,
            direction: KagiDirection::Down,
        };
        let dreamrunner = Dreamrunner {
            k_rev,
            k_src,
            ma_src
        };
        let mut data = Vec::new();

        let candles = self.candles.clone();
        for candle in candles {
            period.push(candle);
            let _ = dreamrunner.signal(&mut kagi, &period)?;
            data.push(Data {
                x: candle.date.to_unix_ms(),
                y: kagi.line
            });
        }
        Ok(data)
    }
    
    pub fn simulate(
        &mut self, 
        capital: f64, 
        wma_period: usize, 
        k_rev: f64, 
        k_src: Source, 
        ma_src: Source
    ) -> anyhow::Result<Vec<Data>> {
        let mut active_trade: Option<Trade> = None;
        let mut period = RollingCandles::new(wma_period + 1);
        let mut kagi = Kagi {
            line: self.candles.first().unwrap().low,
            direction: KagiDirection::Down,
        };
        let dreamrunner = Dreamrunner {
            k_rev,
            k_src,
            ma_src
        };
        let mut data = Vec::new();
        let mut wmas = Vec::new();
        let mut kagis = Vec::new();
        
        let mut capital = capital;
        
        let candles = self.candles.clone();
        for candle in candles {
            period.push(candle);
     
            let signal = dreamrunner.signal(&mut kagi, &period)?;

            let period_from_curr: Vec<&Candle> = period.vec.range(0..period.vec.len() - 1).collect();
            wmas.push(Data {
                x: candle.date.to_unix_ms(),
                y: dreamrunner.wma(&period_from_curr)
            });
            kagis.push(Data {
                x: candle.date.to_unix_ms(),
                y: kagi.line
            });
            
            match signal {
                Signal::Long((price, time)) => {
                    match &active_trade {
                        Some(trade) => {
                            if trade.side == Order::Long {
                                continue;
                            }
                        }
                        None => ()
                    }
                    let quantity = capital / price;
                    let trade = Trade {
                        date: time,
                        side: Order::Long,
                        quantity,
                        price: candle.close,
                        capital
                    };
                    active_trade = Some(trade.clone());
                    self.add_trade(trade);
                    data.push(Data {
                        x: time.to_unix_ms(),
                        y: capital
                    });
                },
                Signal::Short((price, time)) => {
                    if let Some(trade) = &active_trade {
                        if trade.side == Order::Short {
                            continue;
                        }
                        let quantity = trade.quantity;
                        capital = quantity * price;
                        
                        let trade = Trade {
                            date: time,
                            side: Order::Short,
                            quantity,
                            price: candle.close,
                            capital,
                        };
                        active_trade = Some(trade.clone());
                        self.add_trade(trade);
                        data.push(Data {
                            x: time.to_unix_ms(),
                            y: capital
                        });
                    }
                },
                Signal::None => ()
            }
        }
        
        Ok(data)
    }
}




#[test]
fn backtest_dreamrunner() -> anyhow::Result<()> {
    use super::*;
    
    let mut backtest = Backtest::new();
    let out_file = "solusdt_15m.csv";
    let csv = PathBuf::from(out_file);

    let start_time = Time::new(2023, &Month::from_num(12), &Day::from_num(16), None, None, None);
    let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(19), None, None, None);
    backtest.add_csv_series(&csv, Some(start_time), Some(end_time))?;

    let k_rev = 0.001;
    let k_src = Source::Close;
    let ma_src = Source::Open;
    let wma_period = 5;
    let capital = 1_000.0;
    
    let capital = backtest.simulate(
        capital,
        wma_period,
        k_rev,
        k_src,
        ma_src
    )?;
    Plot::plot(
        vec![capital],
        "dreamrunner_backtest.png",
        "Dreamrunner Backtest",
        "USDT Profit"
    )?;
    
    let wmas = backtest.wmas(wma_period, k_rev, k_src, ma_src)?;
    let kagis = backtest.kagis(wma_period, k_rev, k_src, ma_src)?;
    Plot::plot(
        vec![wmas, kagis],
        "strategy.png",
        "Strategy",
        "USDT Price"
    )?;

    let summary = backtest.summarize()?;
    println!("{:#?}", summary);
    println!("candles: {}", backtest.candles.len());
    
    Ok(())
}

use simplelog::{
    ColorChoice, Config as SimpleLogConfig, TermLogger,
    TerminalMode,
};
pub fn init_logger() -> anyhow::Result<()> {
    Ok(TermLogger::init(
        LevelFilter::Info,
        SimpleLogConfig::default(),
        TerminalMode::Mixed,
        ColorChoice::Always,
    )?)
}
