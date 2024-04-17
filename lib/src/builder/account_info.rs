#![allow(clippy::result_large_err)]

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};
use crate::DreamrunnerResult;

pub struct AccountInfo {
    recv_window: u32,
}

pub struct AccountInfoBuilder {
    pub request: String,
    pub btree: BTreeMap<String, String>,
}

impl AccountInfo {
    pub fn request(recv_window: Option<u32>) -> AccountInfoBuilder {
        let recv_window = recv_window.unwrap_or(10_000);
        let me = Self { recv_window };
        me.create_request()
    }

    pub fn get_timestamp() -> DreamrunnerResult<u64> {
        let system_time = SystemTime::now();
        let since_epoch = system_time
            .duration_since(UNIX_EPOCH)
            .expect("System time is before UNIX EPOCH");
        Ok(since_epoch.as_secs() * 1000 + u64::from(since_epoch.subsec_nanos()) / 1_000_000)
    }

    fn build(&self) -> BTreeMap<String, String> {
        let mut btree = BTreeMap::<String, String>::new();
        let timestamp = Self::get_timestamp().expect("Failed to get timestamp");
        btree.insert("timestamp".to_string(), timestamp.to_string());
        btree.insert("recvWindow".to_string(), self.recv_window.to_string());
        btree
    }

    fn create_request(&self) -> AccountInfoBuilder {
        let btree = self.build();
        let mut request = String::new();
        for (key, value) in btree.iter() {
            request.push_str(&format!("{}={}&", key, value));
        }
        request.pop();
        AccountInfoBuilder { request, btree }
    }
}
