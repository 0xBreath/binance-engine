

#[derive(Debug, Clone)]
pub enum Interval {
  OneMinute,
  ThreeMinutes,
  FiveMinutes,
  FifteenMinutes,
  ThirtyMinutes,
  OneHour,
  TwoHours,
  FourHours,
  SixHours,
  EightHours,
  TwelveHours,
  OneDay,
  ThreeDays,
  OneWeek,
  OneMonth,
}

impl Interval {
  pub fn as_str(&self) -> String {
    match self {
      Interval::OneMinute => "1m".to_string(),
      Interval::ThreeMinutes => "3m".to_string(),
      Interval::FiveMinutes => "5m".to_string(),
      Interval::FifteenMinutes => "15m".to_string(),
      Interval::ThirtyMinutes => "30m".to_string(),
      Interval::OneHour => "1h".to_string(),
      Interval::TwoHours => "2h".to_string(),
      Interval::FourHours => "4h".to_string(),
      Interval::SixHours => "6h".to_string(),
      Interval::EightHours => "8h".to_string(),
      Interval::TwelveHours => "12h".to_string(),
      Interval::OneDay => "1d".to_string(),
      Interval::ThreeDays => "3d".to_string(),
      Interval::OneWeek => "1w".to_string(),
      Interval::OneMonth => "1M".to_string(),
    }
  }
  
  
  pub fn minutes(&self) -> u32 {
    match self {
      Interval::OneMinute => 1,
      Interval::ThreeMinutes => 3,
      Interval::FiveMinutes => 5,
      Interval::FifteenMinutes => 15,
      Interval::ThirtyMinutes => 30,
      Interval::OneHour => 60,
      Interval::TwoHours => 120,
      Interval::FourHours => 240,
      Interval::SixHours => 360,
      Interval::EightHours => 480,
      Interval::TwelveHours => 720,
      Interval::OneDay => 1440,
      Interval::ThreeDays => 4320,
      Interval::OneWeek => 10080,
      Interval::OneMonth => 43200,
    }
  }
}