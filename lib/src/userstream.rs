use crate::api::{Spot, API};
use crate::client::Client;
use crate::model::{Success, UserDataStream};
use log::*;
use crate::DreamrunnerResult;

#[derive(Clone)]
pub struct UserStream {
    pub client: Client
}

impl UserStream {
    pub async fn start(&self) -> DreamrunnerResult<UserDataStream> {
        self.client.post(API::Spot(Spot::UserDataStream)).await
    }
    
    pub async fn keep_alive(&self, listen_key: &str) -> DreamrunnerResult<Success> {
        debug!("Keep user stream alive");
        self.client.put(API::Spot(Spot::UserDataStream), listen_key).await
    }

    pub async fn close(&self, listen_key: &str) -> DreamrunnerResult<Success> {
        warn!("Closing user stream");
        self.client
            .delete(API::Spot(Spot::UserDataStream), listen_key).await
    }
}
