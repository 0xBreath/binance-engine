use lib::*;
use log::*;
use simplelog::{
  ColorChoice, CombinedLogger, Config as SimpleLogConfig, ConfigBuilder, TermLogger,
  TerminalMode, WriteLogger,
};
use std::fs::File;
use std::path::PathBuf;

pub fn init_logger(log_file: &PathBuf) -> anyhow::Result<()> {
  Ok(CombinedLogger::init(vec![
    TermLogger::new(
      LevelFilter::Info,
      SimpleLogConfig::default(),
      TerminalMode::Mixed,
      ColorChoice::Always,
    ),
    WriteLogger::new(
      LevelFilter::Info,
      ConfigBuilder::new().set_time_format_rfc3339().build(),
      File::create(log_file)?,
    ),
  ])?)
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