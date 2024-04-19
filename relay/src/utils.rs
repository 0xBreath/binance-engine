use lib::*;
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
  )?)
}

pub fn is_testnet() -> DreamrunnerResult<bool> {
  std::env::var("TESTNET")?
    .parse::<bool>()
    .map_err(DreamrunnerError::ParseBool)
}

pub fn disable_trading() -> DreamrunnerResult<bool> {
  std::env::var("DISABLE_TRADING")?
    .parse::<bool>()
    .map_err(DreamrunnerError::ParseBool)
}