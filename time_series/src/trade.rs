use crate::Time;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub enum Source {
  Open,
  High,
  Low,
  Close
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Signal {
  Long((f64, Time)),
  Short((f64, Time)),
  None
}

impl Signal {
  pub fn print(&self) -> String {
    match self {
      Signal::Long(data) => {
        format!("🟢 Long {}", data.0)
      },
      Signal::Short(data) => {
        format!("🔴️ Short {}", data.0)
      },
      Signal::None => "No signal".to_string()
    }
  }

  #[allow(dead_code)]
  pub fn price(&self) -> Option<f64> {
    match self {
      Signal::Long((price, _)) => Some(*price),
      Signal::Short((price, _)) => Some(*price),
      Signal::None => None
    }
  }
}