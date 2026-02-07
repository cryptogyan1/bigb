mod client;
mod config;
mod domain;
mod execution;
mod monitor;
mod strategy;
mod ws;
mod cache;
mod wallet;

use anyhow::Result;
use clap::Parser;
use config::{Args, Config};
use log::{info, warn};
use std::sync::Arc;

use client::PolymarketClient;
use execution::Trader;
use monitor::MarketMonitor;
use strategy::ArbitrageDetector;
use wallet::signer::WalletSigner;
use cache::PriceCache;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    env_logger::init();

    info!("🚀 Starting Polymarket Arbitrage Bot");

    let args = Args::parse();
    let config = Config::load(&args.config)?;

    // ==================================================
    // API CLIENT
    // ==================================================
    let api = Arc::new(PolymarketClient::new(
        config.polymarket.gamma_api_url.clone(),
        config.polymarket.clob_api_url.clone(),
        config
            .polymarket
            .api_key
            .clone()
            .expect("POLY_API_KEY missing"),
        std::env::var("POLY_API_SECRET")
            .expect("POLY_API_SECRET missing"),
        std::env::var("POLY_API_PASSPHRASE")
            .expect("POLY_API_PASSPHRASE missing"),
        false,
    ));

    // ==================================================
    // WALLET + BALANCE LOGGING
    // ==================================================
    let signer = if let Some(pk) = &config.wallet.private_key {
        let signer = WalletSigner::new(pk, config.wallet.chain_id)?;

        info!("🔑 Wallet loaded");
        info!("🧾 Signer wallet: {}", signer.address());
        info!("🧾 Proxy wallet: {}", config.wallet.proxy_wallet);

        match api.get_usdc_balance().await {
            Ok(balance) => info!(
                "💰 USDC balance (API scope): {}",
                balance
            ),
            Err(e) => warn!("Failed to fetch USDC balance: {}", e),
        }

        Some(signer)
    } else {
        warn!("⚠️ No wallet private key provided — trading disabled");
        None
    };

    // ==================================================
    // MARKET DISCOVERY
    // ==================================================
    let (eth_market, btc_market) = discover_markets(&api).await?;

    info!("ETH Market: {}", eth_market.slug);
    info!("BTC Market: {}", btc_market.slug);

    // ==================================================
    // PRICE CACHE + TOKEN IDS
    // ==================================================
    let price_cache = PriceCache::new();
    let mut token_ids = Vec::new();

    for group in &eth_market.tokens {
    for t in group {
        token_ids.push(t.token_id.clone());
    }
}

for group in &btc_market.tokens {
    for t in group {
        token_ids.push(t.token_id.clone());
    }
}

    // ==================================================
    // WEBSOCKET
    // ==================================================
    {
        let cache = price_cache.clone();
        let ws_url = config.polymarket.ws_url.clone();

        tokio::spawn(async move {
            ws::start_ws(ws_url, cache, token_ids).await;
        });
    }

    // ==================================================
    // MONITOR
    // ==================================================
    let monitor = Arc::new(MarketMonitor::new(
        api.clone(),
        eth_market,
        btc_market,
        config.trading.check_interval_ms,
        price_cache.clone(),
    ));

    // ==================================================
    // STRATEGY + TRADER
    // ==================================================
    let detector = Arc::new(
        ArbitrageDetector::new(config.trading.min_profit_threshold),
    );

    let trader = Arc::new(Trader::new(
        api.clone(),
        config.trading.clone(),
        config.wallet.clone(),
        signer,
    ));

    // ==================================================
    // MAIN LOOP
    // ==================================================
    monitor
        .start_monitoring({
            let detector = detector.clone();
            let trader = trader.clone();

            move |snapshot| {
                let detector = detector.clone();
                let trader = trader.clone();

                async move {
                    let opportunities =
                        detector.detect_opportunities(&snapshot);

                    for opportunity in opportunities {
                        let _ = trader.execute_arbitrage(&opportunity).await;
                    }
                }
            }
        })
        .await;

    Ok(())
}

// ==================================================
// MARKET DISCOVERY
// ==================================================
async fn discover_markets(
    api: &PolymarketClient,
) -> Result<(domain::Market, domain::Market)> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();

    let mut seen = std::collections::HashSet::new();

    let eth = discover_market(api, "ETH", "eth", now, &mut seen).await?;
    seen.insert(eth.condition_id.clone());

    let btc = discover_market(api, "BTC", "btc", now, &mut seen).await?;

    Ok((eth, btc))
}

async fn discover_market(
    api: &PolymarketClient,
    name: &str,
    prefix: &str,
    now: u64,
    seen: &mut std::collections::HashSet<String>,
) -> Result<domain::Market> {
    let base = (now / 900) * 900;

    for i in 0..=3 {
        let ts = base - i * 900;
        let slug = format!("{}-updown-15m-{}", prefix, ts);

        if let Ok(market) = api.get_market_by_slug(&slug).await {
            if !seen.contains(&market.condition_id)
                && market.active
                && !market.closed
            {
                info!("Found {} market: {}", name, market.slug);
                return Ok(market);
            }
        }
    }

    anyhow::bail!("No active {} market found", name)
}
