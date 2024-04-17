use crate::api::{Spot, API};
use crate::client::Client;
use crate::model::{Success, UserDataStream};
use log::*;
use crate::DreamrunnerResult;

#[derive(Clone)]
pub struct UserStream {
    pub client: Client,
    pub recv_window: u64,
}

impl UserStream {
    // User Stream
    pub async fn start(&self) -> DreamrunnerResult<UserDataStream> {
        self.client.post(API::Spot(Spot::UserDataStream)).await
    }

    // Current open orders on a symbol
    pub async fn keep_alive(&self, listen_key: &str) -> DreamrunnerResult<Success> {
        info!("Keep user data stream alive");
        self.client.put(API::Spot(Spot::UserDataStream), listen_key).await
    }

    pub async fn close(&self, listen_key: &str) -> DreamrunnerResult<Success> {
        warn!("Closing user data stream");
        self.client
            .delete(API::Spot(Spot::UserDataStream), listen_key).await
    }
}
