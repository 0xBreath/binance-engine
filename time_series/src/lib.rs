pub mod candle;
pub mod trunc;
pub mod square_of_nine;
pub mod time;
pub mod plot;
pub mod trade;
pub mod kagi;
pub mod data;
pub mod data_cache;

pub use candle::*;
pub use time::*;
pub use plot::*;
pub use trade::*;
pub use kagi::*;
pub use data::*;
pub use data_cache::*;

use log::*;
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
  )?
  )
}
