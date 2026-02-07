use crate::client::{PolymarketClient, SignedOrderPayload};
use crate::config::{TradingConfig, WalletConfig};
use crate::domain::*;
use crate::wallet::signer::{ClobOrder, WalletSigner};

use anyhow::{anyhow, Result};
use log::info;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

use ethers::types::{H256, U256};
use ethers::utils::keccak256;

// ==================================================
// Helpers
// ==================================================

fn str_to_h256(s: &str) -> H256 {
    H256::from_slice(&keccak256(s.as_bytes()))
}

fn to_u256_scaled(value: &str) -> U256 {
    let v: f64 = value.parse().unwrap_or(0.0);
    U256::from((v * 1_000_000.0) as u128)
}

// ==================================================

#[derive(Clone)]
struct CachedMarketData {
    market: MarketDetails,
    cached_at: Instant,
}

pub struct Trader {
    api: Arc<PolymarketClient>,
    config: TradingConfig,
    wallet: WalletConfig,
    signer: Option<WalletSigner>,

    total_profit: Arc<Mutex<f64>>,
    trades_executed: Arc<Mutex<u64>>,
    pending_trades: Arc<Mutex<HashMap<String, PendingTrade>>>,
    market_cache: Arc<Mutex<HashMap<String, CachedMarketData>>>,
    live_usdc_balance: Arc<Mutex<rust_decimal::Decimal>>,
}

impl Trader {
    // ==================================================
    // CONSTRUCTOR
    // ==================================================
    pub fn new(
        api: Arc<PolymarketClient>,
        config: TradingConfig,
        wallet: WalletConfig,
        signer: Option<WalletSigner>,
    ) -> Self {
        Self {
            api,
            config,
            wallet,
            signer,
            total_profit: Arc::new(Mutex::new(0.0)),
            trades_executed: Arc::new(Mutex::new(0)),
            pending_trades: Arc::new(Mutex::new(HashMap::new())),
            market_cache: Arc::new(Mutex::new(HashMap::new())),
            live_usdc_balance: Arc::new(Mutex::new(rust_decimal::Decimal::ZERO)),
        }
    }

    // ==================================================
    // BALANCE
    // ==================================================
    pub async fn refresh_balance(&self) -> Result<()> {
        let balance = self.api.get_usdc_balance().await?;
        *self.live_usdc_balance.lock().await = balance;

        info!("💰 USDC balance updated: {}", balance);
        Ok(())
    }

    // ==================================================
    // EXECUTION
    // ==================================================
    pub async fn execute_arbitrage(
        &self,
        opportunity: &ArbitrageOpportunity,
    ) -> Result<()> {
        let signer = self
            .signer
            .as_ref()
            .ok_or_else(|| anyhow!("Wallet signer missing"))?;

        let eth_market = self.api.get_market(&opportunity.eth_condition_id).await?;
        let btc_market = self.api.get_market(&opportunity.btc_condition_id).await?;

        if !eth_market.accepting_orders || !btc_market.accepting_orders {
            info!("⛔ Trade blocked — market closed");
            return Ok(());
        }

        self.refresh_balance().await?;

        let position_size = self.calculate_position_size(opportunity);
        if position_size <= 0.0 {
            info!("⛔ Zero-size trade skipped");
            return Ok(());
        }

        let size_str = format!("{:.6}", position_size);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        info!(
            "🚀 EXECUTING ARB | cost={} profit={}",
            opportunity.total_cost,
            opportunity.expected_profit
        );

        // ================= ETH =================
        let eth_sig = signer
            .sign_order(&ClobOrder {
                token_id: str_to_h256(&opportunity.eth_up_token_id),
                side: 0,
                price: to_u256_scaled(&opportunity.eth_up_price.to_string()),
                size: to_u256_scaled(&size_str),
                expiration: U256::from(now + 300),
                nonce: U256::from(now),
            })
            .await?;

        let eth_payload = SignedOrderPayload {
            order: OrderRequest {
                token_id: opportunity.eth_up_token_id.clone(),
                side: "BUY".into(),
                size: size_str.clone(),
                price: opportunity.eth_up_price.to_string(),
                order_type: "LIMIT".into(),
            },
            signature: eth_sig.to_string(),
            address: self.wallet.proxy_wallet.clone(),
        };

        // ================= BTC =================
        let btc_sig = signer
            .sign_order(&ClobOrder {
                token_id: str_to_h256(&opportunity.btc_down_token_id),
                side: 0,
                price: to_u256_scaled(&opportunity.btc_down_price.to_string()),
                size: to_u256_scaled(&size_str),
                expiration: U256::from(now + 300),
                nonce: U256::from(now + 1),
            })
            .await?;

        let btc_payload = SignedOrderPayload {
            order: OrderRequest {
                token_id: opportunity.btc_down_token_id.clone(),
                side: "BUY".into(),
                size: size_str,
                price: opportunity.btc_down_price.to_string(),
                order_type: "LIMIT".into(),
            },
            signature: btc_sig.to_string(),
            address: self.wallet.proxy_wallet.clone(),
        };

        // ✅ SAFE async execution
        let _ = tokio::join!(
            self.api.place_signed_order(&eth_payload),
            self.api.place_signed_order(&btc_payload),
        );

        Ok(())
    }

    fn calculate_position_size(&self, opportunity: &ArbitrageOpportunity) -> f64 {
        let max_usd = self.config.max_position_size;
        let cost = f64::try_from(opportunity.total_cost).unwrap_or(1.0);

        if cost <= 0.0 {
            return 0.0;
        }

        (max_usd / cost).floor()
    }
}
