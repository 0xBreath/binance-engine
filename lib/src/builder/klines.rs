use std::collections::BTreeMap;

pub struct Klines {
  /// Ticker symbol (e.g. BTCUSDC)
  pub symbol: String,
  /// Interval of the klines (e.g. 1m, 5m, 15m, 1h, 1d)
  pub interval: String,
  /// Limit of the klines (default 1000)
  pub limit: u16,
  pub start_time: Option<i64>,
  pub end_time: Option<i64>,
}

impl Klines {
  pub fn request(
    symbol: String, 
    interval: String, 
    limit: Option<u16>, 
    start_time: Option<i64>, 
    end_time: Option<i64>
  ) -> String {
    let limit = limit.unwrap_or(1000);
    let me = Self { symbol, interval, limit, start_time, end_time };
    me.create_request()
  }

  fn build(&self) -> BTreeMap<String, String> {
    let mut btree = BTreeMap::<String, String>::new();
    btree.insert("symbol".to_string(), self.symbol.to_string());
    if let Some(start_time) = self.start_time {
      btree.insert("startTime".to_string(), start_time.to_string());
    }
    if let Some(end_time) = self.end_time {
      btree.insert("endTime".to_string(), end_time.to_string());
    }
    btree.insert("interval".to_string(), self.interval.to_string());
    btree.insert("limit".to_string(), self.limit.to_string());
    btree
  }

  fn create_request(&self) -> String {
    let btree = self.build();
    let mut request = String::new();
    for (key, value) in btree.iter().rev() {
      request.push_str(&format!("{}={}&", key, value));
    }
    request.pop();
    println!("{}", request);
    request
  }
}
