use crate::Candle;

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
        // 260% return in 7+ month
        let src = candle.low;
        let diff = candle.close - kagi.line;
        
        // 70% return in 7+ months
        // let src = candle.close;
        // let diff = candle.low - kagi.line;
        
        if diff.abs() > rev_amt {
          if diff > 0.0 {
            new_kagi.line = src;
          }
          if diff < 0.0 {
            new_kagi = Kagi {
              line: src,
              direction: KagiDirection::Down,
            };
          }
        }
      },
      KagiDirection::Down => {
        // 260% return in 7+ month
        let src = candle.high;
        let diff = candle.close - kagi.line;
        
        // 70% return in 7+ months
        // let src = candle.close;
        // let diff = candle.high - kagi.line;
        
        if diff.abs() > rev_amt {
          if diff < 0.0 {
            new_kagi.line = src;
          }
          if diff > 0.0 {
            new_kagi = Kagi {
              line: src,
              direction: KagiDirection::Up,
            };
          }
        }
      },
    }
    
    new_kagi
  }
}