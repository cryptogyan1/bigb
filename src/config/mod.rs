use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/* =======================
WALLET CONFIG
======================= */

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletConfig {
    pub private_key: Option<String>,
    pub chain_id: u64,

    // Polymarket trading wallet (proxy / funder address)
    pub proxy_wallet: String,
}

/* =======================
CLI ARGS
======================= */

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Configuration file path
    #[arg(short, long, default_value = "config.json")]
    pub config: PathBuf,
}

/* =======================
MAIN CONFIG
======================= */

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub polymarket: PolymarketConfig,
    pub trading: TradingConfig,
    pub wallet: WalletConfig,
}

/* =======================
POLYMARKET CONFIG
======================= */

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolymarketConfig {
    pub gamma_api_url: String,
    pub clob_api_url: String,
    pub ws_url: String,

    // CLOB API credentials (REQUIRED for real trading)
    pub api_key: Option<String>,
    pub api_secret: Option<String>,
    pub api_passphrase: Option<String>,
}

/* =======================
TRADING CONFIG
======================= */

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingConfig {
    pub min_profit_threshold: f64,
    pub max_position_size: f64,

    // Optional manual overrides
    pub eth_condition_id: Option<String>,
    pub btc_condition_id: Option<String>,

    pub check_interval_ms: u64,
}

/* =======================
DEFAULT CONFIG
======================= */

impl Default for Config {
    fn default() -> Self {
        Self {
            polymarket: PolymarketConfig {
                gamma_api_url: "https://gamma-api.polymarket.com".to_string(),
                clob_api_url: "https://clob.polymarket.com".to_string(),
                ws_url: "wss://ws-subscriptions-clob.polymarket.com/ws/market".to_string(),

                api_key: None,
                api_secret: None,
                api_passphrase: None,
            },
            trading: TradingConfig {
                min_profit_threshold: 0.01,
                max_position_size: 100.0,
                eth_condition_id: None,
                btc_condition_id: None,
                check_interval_ms: 1000,
            },
            wallet: WalletConfig {
                private_key: None,
                chain_id: 137, // Polygon
                proxy_wallet: String::new(),
            },
        }
    }
}

/* =======================
LOAD / CREATE CONFIG
======================= */

impl Config {
    pub fn load(path: &PathBuf) -> anyhow::Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            let config = Config::default();
            let content = serde_json::to_string_pretty(&config)?;
            std::fs::write(path, content)?;
            Ok(config)
        }
    }
}
