use std::str::FromStr;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use crate::{Candle, Time};

pub const RAPID_API_URL: &str = "https://twelve-data1.p.rapidapi.com";
pub const RAPID_API_KEY: &str = "687a11f943msh17ef6a6b5f4da77p11b9dajsnd13fdf102ceb";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RapidApi {
  pub symbol: String
}

#[derive(Debug, Clone)]
pub enum Interval {
  OneMinute,
  FiveMinutes,
  FifteenMinutes,
  ThirtyMinutes,
  FourtyFiveMinutes,
  OneHour,
  TwoHour,
  FourHour,
  Daily,
  Weekly,
  Monthly,
}

impl Interval {
  pub fn to_str(&self) -> &str {
    match self {
      Interval::OneMinute => "1min",
      Interval::FiveMinutes => "5min",
      Interval::FifteenMinutes => "15min",
      Interval::ThirtyMinutes => "30min",
      Interval::FourtyFiveMinutes => "45min",
      Interval::OneHour => "1h",
      Interval::TwoHour => "2h",
      Interval::FourHour => "4h",
      Interval::Daily => "1day",
      Interval::Weekly => "1week",
      Interval::Monthly => "1month",
    }
  }
}

impl RapidApi {
  pub fn new(symbol: String) -> Self {
    Self { symbol }
  }

  pub async fn query(&self, interval: Interval) -> Vec<Candle> {
    let client = Client::new();
    // output size accepts values [1, 5000] inclusive
    let output_size = 5000;

    let url = format!(
      "{}/time_series?symbol={}&interval={}&outputsize={}&format=json",
      RAPID_API_URL,
      &self.symbol,
      interval.to_str(),
      output_size
    );

    let response = client
      .get(url)
      .header("X-RapidAPI-Key",RAPID_API_KEY)
      .header("X-RapidAPI-Host", "twelve-data1.p.rapidapi.com")
      .send()
      .await
      .expect("Failed to send RapidApi request")
      .text()
      .await
      .expect("Failed to read RapidApi response into text");
    let json: serde_json::Value = serde_json::from_str(&response)
      .expect("Failed to parse RapidApi response into JSON");

    let values = json["values"].as_array().unwrap();
    let mut candles = Vec::<Candle>::new();
    for value in values.iter() {
      let date = Time::from_api_format(value["datetime"].as_str().unwrap());

      let mut volume = None;
      match value["volume"].as_str() {
        None => println!("date no volume: {}", date.to_string()),
        Some(vol) => volume = Some(f64::from_str(vol).expect("Failed to parse open price to f64"))
      };

      candles.push(Candle {
        date,
        close: f64::from_str(value["close"].as_str().unwrap()).expect("Failed to parse close price to f64"),
        high: f64::from_str(value["high"].as_str().unwrap()).expect("Failed to parse high price to f64"),
        low: f64::from_str(value["low"].as_str().unwrap()).expect("Failed to parse low price to f64"),
        open: f64::from_str(value["open"].as_str().unwrap()).expect("Failed to parse open price to f64"),
        volume,
      });
    }
    println!("{} candles retrieved from RapidApi for symbol {}", candles.len(), &self.symbol);
    candles
  }
}