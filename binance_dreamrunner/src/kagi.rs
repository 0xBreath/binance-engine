use time_series::Candle;

#[derive(Debug, Clone)]
pub struct Kagi {
  /// -1 down, 1 up
  pub direction: i8,
  pub line: f64,
}

impl Kagi {
  pub fn new(rev_amt: f64, curr: &Candle, prev: &Candle) -> Self {
    let bull_src = curr.high;
    let bear_src = curr.low;
    let mut line = prev.close;
    let mut direction = if curr.close > line { 1 } else { -1 };

    if direction == 1 { // Current direction is up
      if bull_src > line {
        line = bull_src; // Move the Kagi line up to new high
      }
      if line - bear_src >= rev_amt { // Check for reversal
        line = bear_src;
        direction = -1;
      }
    } else { // Current direction is down
      if bear_src < line {
        line = bear_src; // Move the Kagi line down to new low
      }
      if bull_src - line >= rev_amt { // Check for reversal
        line = bull_src;
        direction = 1;
      }
    }
    Self {
      direction,
      line,
    }
  }
}