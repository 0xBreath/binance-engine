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
        let src = candle.low;
        let diff = candle.close - kagi.line;
        
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
        let src = candle.high;
        let diff = candle.close - kagi.line;
        
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