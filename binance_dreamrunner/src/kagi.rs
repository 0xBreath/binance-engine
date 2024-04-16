use time_series::Candle;

#[derive(Debug, Clone, Copy)]
pub enum KagiDirection {
  Up,
  Down,
}

#[derive(Debug, Clone, Copy)]
pub struct Kagi {
  pub direction: KagiDirection,
  pub line: f64,
}

impl Kagi {
  pub fn update(kagi: &Kagi, rev_amt: f64, candle: &Candle) -> Self {
    let mut new_kagi = *kagi;

    match kagi.direction {
      KagiDirection::Up => {
        if candle.high > kagi.line {
          new_kagi.line = candle.high;
        }
        if kagi.line - candle.low >= rev_amt {
          new_kagi = Kagi {
            line: candle.low,
            direction: KagiDirection::Down,
          };
        }
      },
      KagiDirection::Down => {
        if candle.low < kagi.line {
          new_kagi.line = candle.low;
        }
        if candle.high - kagi.line >= rev_amt {
          new_kagi = Kagi {
            line: candle.high,
            direction: KagiDirection::Up,
          };
        }
      },
    }
    new_kagi
  }
}