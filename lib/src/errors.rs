use log::error;
use serde::Deserialize;
use std::env::VarError;
use std::num::ParseFloatError;
use std::str::ParseBoolError;
use std::time::SystemTimeError;

use actix_web::{error::ResponseError, http::StatusCode, HttpResponse};
use thiserror::Error;
use crate::{ChannelMsg, WebSocketEvent};

pub type DreamrunnerResult<T> = Result<T, DreamrunnerError>;

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Error)]
pub enum DreamrunnerError {
    #[error("BinanceContentError: {0}")]
    Binance(BinanceContentError),
    #[error("KlineMissing")]
    KlineMissing,
    #[error("NoActiveOrder")]
    NoActiveOrder,
    #[error("SideInvalid")]
    SideInvalid,
    #[error("OrderTypeInvalid")]
    OrderTypeInvalid,
    #[error("WebSocketDisconnected")]
    WebSocketDisconnected,
    #[error("Reqwest: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("InvalidHeader: {0}")]
    InvalidHeader(#[from] reqwest::header::InvalidHeaderValue),
    #[error("Io: {0}")]
    Io(#[from] std::io::Error),
    #[error("ParseFloat: {0}")]
    ParseFloat(#[from] ParseFloatError),
    #[error("ParseBool: {0}")]
    ParseBool(#[from] ParseBoolError),
    #[error("UrlParser: {0}")]
    UrlParser(#[from] url::ParseError),
    #[error("Json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Tungstenite: {0}")]
    Tungstenite(#[from] tungstenite::Error),
    #[error("TokioTungstenite: {0}")]
    TokioTungstenite(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("OrderStatusParseError: {0}")]
    OrderStatusParseError(String),
    #[error("Custom: {0}")]
    Custom(String),
    #[error("SystemTime: {0}")]
    SystemTime(#[from] SystemTimeError),
    #[error("EnvMissing: {0}")]
    EnvMissing(#[from] VarError),
    #[error("ExitHandlersInitializedEarly")]
    ExitHandlersInitializedEarly,
    #[error("ExitHandlersNotBothInitialized")]
    ExitHandlersNotBothInitialized,
    #[error("ParseInt: {0}")]
    ParseInt(#[from] std::num::ParseIntError),
    #[error("Anyhow: {0}")]
    Anyhow(#[from] anyhow::Error),
    #[error("SendWebsocketEventError: {0}")]
    SendWebsocketEventError(#[from] crossbeam::channel::SendError<WebSocketEvent>),
    #[error("SendChannelMsgError: {0}")]
    SendChannelMsgError(#[from] crossbeam::channel::SendError<ChannelMsg>),
    #[error("TimeError: {0}")]
    TimeError(#[from] time_series::time::TimeError),
    #[error("Request payload size is too large")]
    Overflow,
    #[error("Payload error: {0}")]
    PayloadError(#[from] actix_web::error::PayloadError),
    #[error("Alert missing price")]
    AlertMissingPrice,
    #[error("JoinError: {0}")]
    JoinError(#[from] tokio::task::JoinError),
}

impl ResponseError for DreamrunnerError {
    fn status_code(&self) -> StatusCode {
        match &self {
            Self::SideInvalid => StatusCode::BAD_REQUEST,
            Self::OrderTypeInvalid => StatusCode::BAD_REQUEST,
            Self::ParseFloat(_) => StatusCode::BAD_REQUEST,
            Self::ParseBool(_) => StatusCode::BAD_REQUEST,
            Self::PayloadError(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).body(self.to_string())
    }
}

#[derive(Debug, Clone, Deserialize, Error)]
pub struct BinanceContentError {
    pub code: i16,
    pub msg: String,
}

impl std::fmt::Display for BinanceContentError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "BinanceContentError: code: {}, msg: {}", self.code, self.msg)
    }
}