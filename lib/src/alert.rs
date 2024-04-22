#![allow(dead_code)]

use std::str::FromStr;
use actix_web::{Result};
use serde::{Serialize, Deserialize};
use crate::WebSocketEvent;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Position {
  Long,
  Short
}

impl FromStr for Position {
  type Err = ();
  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "Long" => Ok(Position::Long),
      "Short" => Ok(Position::Short),
      _ => Err(()),
    }
  }
}

impl Position {
  pub fn as_str(&self) -> &str {
    match self {
      Position::Long => "Long",
      Position::Short => "Short",
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
  pub position: Position,
  #[serde(default)]
  pub price: Option<f64>,
  pub timestamp: i64
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChannelMsg {
  Websocket(WebSocketEvent),
  Alert(Alert)
}