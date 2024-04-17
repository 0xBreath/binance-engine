use std::collections::BTreeMap;
use std::io::Result;
use std::time::{SystemTime, UNIX_EPOCH};

#[allow(dead_code)]
pub struct AllOrders {}

impl AllOrders {
    #[allow(dead_code)]
    pub fn request(symbol: String, recv_window: Option<u32>) -> String {
        Self::create_request(symbol, recv_window)
    }

    pub fn get_timestamp() -> Result<u64> {
        let system_time = SystemTime::now();
        let since_epoch = system_time
            .duration_since(UNIX_EPOCH)
            .expect("System time is before UNIX EPOCH");
        Ok(since_epoch.as_secs() * 1000 + u64::from(since_epoch.subsec_nanos()) / 1_000_000)
    }

    #[allow(dead_code)]
    fn build(symbol: String, recv_window: Option<u32>) -> BTreeMap<String, String> {
        let mut btree = BTreeMap::<String, String>::new();
        btree.insert("symbol".to_string(), symbol);
        let timestamp = Self::get_timestamp().expect("Failed to get timestamp");
        btree.insert("timestamp".to_string(), timestamp.to_string());
        if let Some(recv_window) = recv_window {
            btree.insert("recvWindow".to_string(), recv_window.to_string());
        }
        btree
    }

    #[allow(dead_code)]
    fn create_request(symbol: String, recv_window: Option<u32>) -> String {
        let btree = Self::build(symbol, recv_window);
        let mut request = String::new();
        for (key, value) in btree.iter() {
            request.push_str(&format!("{}={}&", key, value));
        }
        request.pop();
        request
    }
}
