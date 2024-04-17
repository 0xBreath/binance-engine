use crate::api::API;
use crate::errors::{DreamrunnerResult};
use crate::{BinanceContentError, DreamrunnerError};
use hex::encode as hex_encode;
use hmac::{Hmac, Mac};
use log::*;
use reqwest::Response;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE, USER_AGENT};
use serde::de::DeserializeOwned;
use sha2::Sha256;

#[derive(Clone)]
pub struct Client {
    api_key: String,
    secret_key: String,
    host: String,
    inner_client: reqwest::Client,
}

impl Client {
    pub fn new(api_key: Option<String>, secret_key: Option<String>, host: String) -> DreamrunnerResult<Self> {
        Ok(Client {
            api_key: api_key.unwrap_or_default(),
            secret_key: secret_key.unwrap_or_default(),
            host,
            inner_client: reqwest::Client::builder()
                .pool_idle_timeout(None)
                .timeout(std::time::Duration::from_secs(10))
                .connect_timeout(std::time::Duration::from_secs(10))
                .build()?,
        })
    }

    pub async fn get_signed<T: DeserializeOwned>(
        &self,
        endpoint: API,
        request: Option<String>,
    ) -> DreamrunnerResult<T> {
        let url = self.sign_request(endpoint, request);
        debug!("url: {}", url);
        let client = &self.inner_client;
        let response = client
            .get(url.as_str())
            .headers(self.build_headers(true)?)
            .send().await?;
        self.handler(response).await
    }

    pub async fn post_signed<T: DeserializeOwned>(&self, endpoint: API, request: String) -> DreamrunnerResult<T> {
        let url = self.sign_request(endpoint, Some(request));
        info!("url: {}", url);
        let client = &self.inner_client;
        let request = client.post(url.as_str()).headers(self.build_headers(true)?);
        let response = request.send().await?;
        self.handler(response).await
    }

    pub async fn delete_signed<T: DeserializeOwned>(
        &self,
        endpoint: API,
        request: Option<String>,
    ) -> DreamrunnerResult<T> {
        let url = self.sign_request(endpoint, request);
        debug!("url: {}", url);
        let client = &self.inner_client;
        let response = client
            .delete(url.as_str())
            .headers(self.build_headers(true)?)
            .send().await?;
        self.handler(response).await
    }

    pub async fn get<T: DeserializeOwned>(&self, endpoint: API, request: Option<String>) -> DreamrunnerResult<T> {
        let mut url: String = format!("{}{}", self.host, String::from(endpoint));
        if let Some(request) = request {
            if !request.is_empty() {
                url.push_str(format!("?{}", request).as_str());
            }
        }
        debug!("url: {}", url);
        let client = &self.inner_client;
        let response = client.get(url.as_str()).send().await?;
        self.handler(response).await
    }

    #[allow(dead_code)]
    pub async fn post<T: DeserializeOwned>(&self, endpoint: API) -> DreamrunnerResult<T> {
        let url: String = format!("{}{}", self.host, String::from(endpoint));
        debug!("url: {}", url);
        let client = &self.inner_client;
        let response = client
            .post(url.as_str())
            .headers(self.build_headers(false)?)
            .send().await?;
        self.handler(response).await
    }

    #[allow(dead_code)]
    pub async fn put<T: DeserializeOwned>(&self, endpoint: API, listen_key: &str) -> DreamrunnerResult<T> {
        let url: String = format!("{}{}", self.host, String::from(endpoint));
        debug!("url: {}", url);
        let data: String = format!("listenKey={}", listen_key);
        let client = &self.inner_client;
        let response = client
            .put(url.as_str())
            .headers(self.build_headers(false)?)
            .body(data)
            .send().await?;
        self.handler(response).await
    }

    #[allow(dead_code)]
    pub async fn delete<T: DeserializeOwned>(&self, endpoint: API, listen_key: &str) -> DreamrunnerResult<T> {
        let url: String = format!("{}{}", self.host, String::from(endpoint));
        debug!("url: {}", url);
        let data: String = format!("listenKey={}", listen_key);
        let client = &self.inner_client;
        let response = client
            .delete(url.as_str())
            .headers(self.build_headers(false)?)
            .body(data)
            .send().await?;
        self.handler(response).await
    }

    // Request must be signed
    fn sign_request(&self, endpoint: API, request: Option<String>) -> String {
        if let Some(request) = request {
            let mut signed_key =
                Hmac::<Sha256>::new_from_slice(self.secret_key.as_bytes()).unwrap();
            signed_key.update(request.as_bytes());
            let signature = hex_encode(signed_key.finalize().into_bytes());
            let request_body: String = format!("{}&signature={}", request, signature);
            format!("{}{}?{}", self.host, String::from(endpoint), request_body)
        } else {
            let signed_key = Hmac::<Sha256>::new_from_slice(self.secret_key.as_bytes()).unwrap();
            let signature = hex_encode(signed_key.finalize().into_bytes());
            let request_body: String = format!("&signature={}", signature);
            format!("{}{}?{}", self.host, String::from(endpoint), request_body)
        }
    }

    fn build_headers(&self, content_type: bool) -> DreamrunnerResult<HeaderMap> {
        let mut custom_headers = HeaderMap::new();
        custom_headers.insert(USER_AGENT, HeaderValue::from_static("binance-rs"));
        if content_type {
            custom_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        }
        custom_headers.insert(
            "x-mbx-apikey",
            HeaderValue::from_str(self.api_key.as_str())?,
        );
        Ok(custom_headers)
    }

    async fn handler<T: DeserializeOwned>(&self, response: Response) -> DreamrunnerResult<T> {
        if response.status().is_success() {
            Ok(response.json::<T>().await?)
        } else {
            let error: BinanceContentError = response.json().await?;
            Err(DreamrunnerError::Binance(error))
        }
    }
}
