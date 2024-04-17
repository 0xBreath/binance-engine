use std::collections::BTreeMap;
use std::io::Result;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct CancelOrder {
    pub order_id: u64,
    /// Ticker symbol (e.g. BTCUSDC)
    pub symbol: String,
    pub recv_window: Option<u32>,
}

impl CancelOrder {
    pub fn request(order_id: u64, symbol: String, recv_window: Option<u32>) -> String {
        let me = Self {
            order_id,
            symbol,
            recv_window,
        };
        me.create_request()
    }

    pub fn get_timestamp() -> Result<u64> {
        let system_time = SystemTime::now();
        let since_epoch = system_time
            .duration_since(UNIX_EPOCH)
            .expect("System time is before UNIX EPOCH");
        Ok(since_epoch.as_secs() * 1000 + u64::from(since_epoch.subsec_nanos()) / 1_000_000)
    }

    fn build(&self) -> BTreeMap<String, String> {
        let mut btree = BTreeMap::<String, String>::new();
        btree.insert("orderId".to_string(), self.order_id.to_string());
        let timestamp = Self::get_timestamp().expect("Failed to get timestamp");
        btree.insert("timestamp".to_string(), timestamp.to_string());
        if let Some(recv_window) = self.recv_window {
            btree.insert("recvWindow".to_string(), recv_window.to_string());
        }
        btree.insert(
            "newClientOrderId".to_string(),
            format!("{}-{}", timestamp, "CANCEL"),
        );
        btree.insert("symbol".to_string(), self.symbol.to_string());
        btree
    }

    fn create_request(&self) -> String {
        let btree = self.build();
        let mut request = String::new();
        for (key, value) in btree.iter() {
            request.push_str(&format!("{}={}&", key, value));
        }
        request.pop();
        request
    }
}
