use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct CachedOrderbook {
    pub bids: Vec<(Decimal, Decimal)>, // (price, size)
    pub asks: Vec<(Decimal, Decimal)>, // (price, size)
    pub last_update_ms: u128,
}

#[derive(Clone)]
pub struct PriceCache {
    inner: Arc<RwLock<HashMap<String, CachedOrderbook>>>,
}

impl PriceCache {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn update(
        &self,
        token_id: &str,
        bids: Vec<(Decimal, Decimal)>,
        asks: Vec<(Decimal, Decimal)>,
    ) {
        let mut map = self.inner.write().await;
        map.insert(
            token_id.to_string(),
            CachedOrderbook {
                bids,
                asks,
                last_update_ms: now_ms(),
            },
        );
    }

    pub async fn get(&self, token_id: &str) -> Option<CachedOrderbook> {
        self.inner.read().await.get(token_id).cloned()
    }
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis()
}

