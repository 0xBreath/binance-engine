use log::error;
use serde::Deserialize;
use std::env::VarError;
use std::num::ParseFloatError;
use std::str::ParseBoolError;
use std::time::SystemTimeError;

use actix_web::{error::ResponseError, http::StatusCode, HttpResponse};
use thiserror::Error;
use crate::WebSocketEvent;

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
    #[error("SendError: {0}")]
    SendError(#[from] crossbeam::channel::SendError<WebSocketEvent>),
}

impl ResponseError for DreamrunnerError {
    fn status_code(&self) -> StatusCode {
        match &self {
            Self::Binance(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::KlineMissing => StatusCode::INTERNAL_SERVER_ERROR,
            Self::NoActiveOrder => StatusCode::INTERNAL_SERVER_ERROR,
            Self::SideInvalid => StatusCode::BAD_REQUEST,
            Self::OrderTypeInvalid => StatusCode::BAD_REQUEST,
            Self::WebSocketDisconnected => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Reqwest(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::InvalidHeader(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::ParseFloat(_) => StatusCode::BAD_REQUEST,
            Self::ParseBool(_) => StatusCode::BAD_REQUEST,
            Self::UrlParser(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Json(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Tungstenite(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::OrderStatusParseError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Custom(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::SystemTime(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::EnvMissing(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::ExitHandlersInitializedEarly => StatusCode::INTERNAL_SERVER_ERROR,
            Self::ExitHandlersNotBothInitialized => StatusCode::INTERNAL_SERVER_ERROR,
            Self::ParseInt(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Anyhow(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::SendError(_) => StatusCode::INTERNAL_SERVER_ERROR,
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