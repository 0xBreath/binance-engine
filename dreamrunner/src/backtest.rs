#![allow(dead_code)]
#![allow(clippy::unnecessary_cast)]

use time_series::{Candle, Data, Kagi, KagiDirection, Pnl, RollingCandles, Signal, Source, Time, trunc};
use std::fs::File;
use std::path::PathBuf;
use std::str::FromStr;
use lib::{Account};
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
        open: f64::from_str(&record[1])?,
        high: f64::from_str(&record[2])?,
        low: f64::from_str(&record[3])?,
        close: f64::from_str(&record[4])?,
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
  
  pub async fn add_klines(&mut self, account: &Account, start_time: Option<Time>, end_time: Option<Time>) -> anyhow::Result<()> {
    let days_back = match (start_time, end_time) {
      (Some(start), Some(end)) => {
        start.diff_days(&end)?
      },
      (Some(start), None) => {
        start.diff_days(&Time::now())?
      },
      _ => 30
    };
    println!("days back: {}", days_back);
    let klines = account.kline_history(days_back).await?;
    for kline in klines.into_iter().rev() {
      self.add_candle(kline.to_candle());
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

  pub fn avg_quote_trade_size(&self) -> anyhow::Result<f64> {
    let avg = self.trades.iter().map(|t| {
      t.price * t.quantity
    }).sum::<f64>() / self.trades.len() as f64;
    Ok(trunc!(avg, 4))
  }

  pub fn pnl(&self) -> anyhow::Result<Pnl> {
    let trades = &self.trades;
    let initial_capital = trades[0].price * trades[0].quantity;
    let mut capital = initial_capital;

    let mut base = 0.0;
    let mut quote = 0.0;
    let mut pct = 0.0;
    let mut pct_data = Vec::new();
    let mut quote_data = Vec::new();
    let mut base_data = Vec::new();
    let mut winners = 0;
    let mut total_trades = 0;
    for trades in trades.windows(2) {
      let exit = &trades[1];
      let entry = &trades[0];
      let factor = match entry.side {
        Order::Long => 1.0,
        Order::Short => -1.0,
      };
      let deci_pnl = (exit.price - entry.price) / entry.price * factor;
      let base_pnl = deci_pnl * entry.quantity;
      let quote_pnl = base_pnl * entry.price;
      capital += quote_pnl;
      base += base_pnl;
      quote += quote_pnl;
      pct = capital / initial_capital * 100.0 - 100.0;
      if quote_pnl > 0.0 {
        winners +=1 ;
      }
      total_trades += 1;

      pct_data.push(Data {
        x: entry.date.to_unix_ms(),
        y: trunc!(pct, 4)
      });
      quote_data.push(Data {
        x: entry.date.to_unix_ms(),
        y: trunc!(quote, 4)
      });
      base_data.push(Data {
        x: entry.date.to_unix_ms(),
        y: trunc!(base, 4)
      });
    }
    let avg_pct_pnl = pct_data.iter().map(|d| d.y).sum::<f64>() / pct_data.len() as f64;
    let win_rate = (winners as f64 / total_trades as f64) * 100.0;
    let max_pct_drawdown = pct_data.iter().fold((0.0, 0.0), |(max, drawdown), d| {
      let max = (max as f64).max(d.y);
      let drawdown = (drawdown as f64).min(d.y - max);
      (max, drawdown)
    }).1;
    Ok(Pnl {
      base: trunc!(base, 4),
      quote: trunc!(quote, 4),
      pct: trunc!(pct, 4),
      pct_data,
      win_rate: trunc!(win_rate, 4),
      avg_quote_trade_size: self.avg_quote_trade_size()?,
      avg_pct_pnl: trunc!(avg_pct_pnl, 4),
      max_pct_drawdown: trunc!(max_pct_drawdown, 4),
      quote_data,
      base_data
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




#[tokio::test]
async fn backtest_dreamrunner() -> anyhow::Result<()> {
  use super::*;
  use time_series::{Day, Month, Plot};
  dotenv::dotenv().ok();

  let account = match std::env::var("TESTNET")?.parse::<bool>()? {
    true => {
      Account {
        client: Client::new(
          Some(std::env::var("BINANCE_TEST_API_KEY")?),
          Some(std::env::var("BINANCE_TEST_API_SECRET")?),
          BINANCE_TEST_API.to_string(),
        )?,
        recv_window: 5000,
        base_asset: BASE_ASSET.to_string(),
        quote_asset: QUOTE_ASSET.to_string(),
        ticker: TICKER.to_string(),
        interval: INTERVAL
      }
    }
    false => {
      Account {
        client: Client::new(
          Some(std::env::var("BINANCE_LIVE_API_KEY")?),
          Some(std::env::var("BINANCE_LIVE_API_SECRET")?),
          BINANCE_LIVE_API.to_string(),
        )?,
        recv_window: 5000,
        base_asset: BASE_ASSET.to_string(),
        quote_asset: QUOTE_ASSET.to_string(),
        ticker: TICKER.to_string(),
        interval: INTERVAL
      }
    }
  };

  let mut backtest = Backtest::new();

  let start_time = Time::new(2023, &Month::from_num(9), &Day::from_num(19), None, None, None);
  let end_time = Time::new(2024, &Month::from_num(4), &Day::from_num(19), None, None, None);

  backtest.add_klines(&account, Some(start_time), Some(end_time)).await?;
  // let out_file = "solusdt_15m.csv";
  // let csv = PathBuf::from(out_file);
  // backtest.add_csv_series(&csv, Some(start_time), Some(end_time))?;

  let earliest = backtest.candles.last().unwrap().date;
  let latest = backtest.candles.first().unwrap().date;
  println!("kline history: {} - {}", earliest.to_string(), latest.to_string());
  

  let k_rev = 0.001;
  let k_src = Source::Close;
  let ma_src = Source::Open;
  let wma_period = 5;
  let capital = 1_000.0;

  let _ = backtest.simulate(
    capital,
    wma_period,
    k_rev,
    k_src,
    ma_src
  )?;
  let summary = backtest.pnl()?;
  println!("{:#?}", summary);
  
  Plot::plot(
    vec![summary.pct_data],
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
  
  println!("candles: {}", backtest.candles.len());

  Ok(())
}
