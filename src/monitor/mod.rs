use crate::client::PolymarketClient;
use crate::domain::*;
use crate::cache::PriceCache;
use anyhow::Result;
use log::{info, warn};
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use rust_decimal::Decimal;

pub struct MarketMonitor {
    api: Arc<PolymarketClient>,
    eth_market: Arc<tokio::sync::Mutex<Market>>,
    btc_market: Arc<tokio::sync::Mutex<Market>>,
    check_interval: Duration,

    eth_up_token_id: Arc<tokio::sync::Mutex<Option<String>>>,
    eth_down_token_id: Arc<tokio::sync::Mutex<Option<String>>>,
    btc_up_token_id: Arc<tokio::sync::Mutex<Option<String>>>,
    btc_down_token_id: Arc<tokio::sync::Mutex<Option<String>>>,

    last_market_refresh: Arc<tokio::sync::Mutex<Option<std::time::Instant>>>,
    current_period_timestamp: Arc<tokio::sync::Mutex<u64>>,

    price_cache: PriceCache,
}

#[derive(Debug, Clone)]
pub struct MarketSnapshot {
    pub eth_market: MarketData,
    pub btc_market: MarketData,
    pub eth_market_meta: MarketMeta,
    pub btc_market_meta: MarketMeta,
    pub timestamp: std::time::Instant,
    
}


#[derive(Debug, Clone)]
pub struct MarketMeta {
    pub name: String,
    pub slug: String,
    pub end_time_unix: u64,
}

impl MarketMonitor {
    pub fn new(
        api: Arc<PolymarketClient>,
        eth_market: Market,
        btc_market: Market,
        check_interval_ms: u64,
        price_cache: PriceCache,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            api,
            eth_market: Arc::new(tokio::sync::Mutex::new(eth_market)),
            btc_market: Arc::new(tokio::sync::Mutex::new(btc_market)),
            check_interval: Duration::from_millis(check_interval_ms),
            price_cache,

            eth_up_token_id: Arc::new(tokio::sync::Mutex::new(None)),
            eth_down_token_id: Arc::new(tokio::sync::Mutex::new(None)),
            btc_up_token_id: Arc::new(tokio::sync::Mutex::new(None)),
            btc_down_token_id: Arc::new(tokio::sync::Mutex::new(None)),

            last_market_refresh: Arc::new(tokio::sync::Mutex::new(None)),
            current_period_timestamp: Arc::new(tokio::sync::Mutex::new((now / 900) * 900)),
        }
    }

    fn end_time_from_slug(slug: &str) -> u64 {
        slug.split('-')
            .last()
            .and_then(|s| s.parse::<u64>().ok())
            .map(|start| start + 900)
            .unwrap_or(0)
    }

    async fn refresh_market_tokens(&self) -> Result<()> {
        let should_refresh = {
            let last = self.last_market_refresh.lock().await;
            last.map(|t| t.elapsed().as_secs() >= 900).unwrap_or(true)
        };

        if !should_refresh {
            return Ok(());
        }

        let eth_id = self.eth_market.lock().await.condition_id.clone();
        let btc_id = self.btc_market.lock().await.condition_id.clone();

        if let Ok(m) = self.api.get_market(&eth_id).await {
            for t in m.tokens {
                let o = t.outcome.to_uppercase();
                if o.contains("UP") || o == "1" {
                    *self.eth_up_token_id.lock().await = Some(t.token_id);
                } else if o.contains("DOWN") || o == "0" {
                    *self.eth_down_token_id.lock().await = Some(t.token_id);
                }
            }
        }

        if let Ok(m) = self.api.get_market(&btc_id).await {
            for t in m.tokens {
                let o = t.outcome.to_uppercase();
                if o.contains("UP") || o == "1" {
                    *self.btc_up_token_id.lock().await = Some(t.token_id);
                } else if o.contains("DOWN") || o == "0" {
                    *self.btc_down_token_id.lock().await = Some(t.token_id);
                }
            }
        }

        *self.last_market_refresh.lock().await = Some(std::time::Instant::now());
        Ok(())
    }

    pub async fn start_monitoring<F, Fut>(&self, on_snapshot: F)
    where
        F: Fn(MarketSnapshot) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        info!("📡 Market monitor running (safe auto-rotation)");

        loop {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            let current = (now / 900) * 900;
            let stored = *self.current_period_timestamp.lock().await;

            if current != stored {
                warn!("⏰ 15m window changed — waiting for new markets");
                *self.current_period_timestamp.lock().await = current;
                *self.last_market_refresh.lock().await = None;
            }

            if let Ok(snapshot) = self.fetch_market_data().await {
                on_snapshot(snapshot).await;
            }

            sleep(self.check_interval).await;
        }
    }

    pub async fn fetch_market_data(&self) -> Result<MarketSnapshot> {
        self.refresh_market_tokens().await?;

        let eth = self.eth_market.lock().await.clone();
        let btc = self.btc_market.lock().await.clone();

        let _usdc_balance = self
    .api
    .get_usdc_balance()
    .await
    .unwrap_or(Decimal::ZERO);

        let eth_up_id = self.eth_up_token_id.lock().await.clone();
        let eth_down_id = self.eth_down_token_id.lock().await.clone();
        let btc_up_id = self.btc_up_token_id.lock().await.clone();
        let btc_down_id = self.btc_down_token_id.lock().await.clone();

        Ok(MarketSnapshot {
            eth_market: MarketData {
                condition_id: eth.condition_id.clone(),
                market_name: "ETH".into(),
                up_token: self.fetch_token_price(&eth_up_id).await,
                down_token: self.fetch_token_price(&eth_down_id).await,
            },
            btc_market: MarketData {
                condition_id: btc.condition_id.clone(),
                market_name: "BTC".into(),
                up_token: self.fetch_token_price(&btc_up_id).await,
                down_token: self.fetch_token_price(&btc_down_id).await,
            },
            eth_market_meta: MarketMeta {
                name: eth.question.clone(),
                slug: eth.slug.clone(),
                end_time_unix: Self::end_time_from_slug(&eth.slug),
            },
            btc_market_meta: MarketMeta {
                name: btc.question.clone(),
                slug: btc.slug.clone(),
                end_time_unix: Self::end_time_from_slug(&btc.slug),
            },
            timestamp: std::time::Instant::now(),
        })
    }

    async fn fetch_token_price(&self, token_id: &Option<String>) -> Option<TokenPrice> {
        let id = token_id.as_ref()?;
        let cached = self.price_cache.get(id).await?;

        Some(TokenPrice {
            token_id: id.clone(),
            bid: cached.bids.first().map(|(p, _)| *p),
            ask: cached.asks.first().map(|(p, _)| *p),

        })
    }
}
