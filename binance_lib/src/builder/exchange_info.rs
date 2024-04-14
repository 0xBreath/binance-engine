use std::collections::BTreeMap;

#[allow(dead_code)]
pub struct ExchangeInfo {}

impl ExchangeInfo {
    #[allow(dead_code)]
    pub fn request(symbol: String) -> String {
        Self::create_request(symbol)
    }

    #[allow(dead_code)]
    fn build(symbol: String) -> BTreeMap<String, String> {
        let mut btree = BTreeMap::<String, String>::new();
        btree.insert("symbol".to_string(), symbol);
        btree
    }

    #[allow(dead_code)]
    fn create_request(symbol: String) -> String {
        let btree = Self::build(symbol);
        let mut request = String::new();
        for (key, value) in btree.iter() {
            request.push_str(&format!("{}={}&", key, value));
        }
        request.pop();
        request
    }
}
