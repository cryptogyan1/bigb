use crate::domain::*;
use crate::monitor::MarketSnapshot;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};

#[derive(Clone)]
pub struct ArbitrageDetector {
    min_profit_threshold: Decimal,
}

impl ArbitrageDetector {
    pub fn new(min_profit_threshold: f64) -> Self {
        Self {
            min_profit_threshold: Decimal::from_f64(min_profit_threshold)
                .unwrap_or(dec!(0)),
        }
    }

    /// TRUE arbitrage only:
    /// ETH_UP + BTC_DOWN < 1
    /// ETH_DOWN + BTC_UP < 1
    pub fn detect_opportunities(
        &self,
        snapshot: &MarketSnapshot,
    ) -> Vec<ArbitrageOpportunity> {
        let mut opportunities = Vec::new();

        // =====================================================
        // ETH UP + BTC DOWN
        // =====================================================
        if let (Some(eth), Some(btc)) = (
            snapshot.eth_market.up_token.as_ref(),
            snapshot.btc_market.down_token.as_ref(),
        ) {
            if let Some(o) = self.build_opportunity(
                eth,
                btc,
                &snapshot.eth_market.condition_id,
                &snapshot.btc_market.condition_id,
            ) {
                opportunities.push(o);
            }
        }

        // =====================================================
        // ETH DOWN + BTC UP
        // =====================================================
        if let (Some(eth), Some(btc)) = (
            snapshot.eth_market.down_token.as_ref(),
            snapshot.btc_market.up_token.as_ref(),
        ) {
            if let Some(o) = self.build_opportunity(
                eth,
                btc,
                &snapshot.eth_market.condition_id,
                &snapshot.btc_market.condition_id,
            ) {
                opportunities.push(o);
            }
        }

        opportunities
    }

    /// Bundle sizing (SAFE, INTEGER ONLY)
    fn build_opportunity(
        &self,
        eth_token: &TokenPrice,
        btc_token: &TokenPrice,
        eth_condition_id: &str,
        btc_condition_id: &str,
    ) -> Option<ArbitrageOpportunity> {
        // -------------------------------------------------
        // USE ASK PRICE (worst-case entry)
        // -------------------------------------------------
        let eth_price = eth_token.ask?;
        let btc_price = btc_token.ask?;

        let bundle_cost = eth_price + btc_price;

        // ❌ NOT arbitrage
        if bundle_cost >= dec!(1.0) {
            return None;
        }

        let profit_per_bundle = dec!(1.0) - bundle_cost;

        if profit_per_bundle < self.min_profit_threshold {
            return None;
        }

        // -------------------------------------------------
        // CAPITAL (TEMP PLACEHOLDER — $10)
        // -------------------------------------------------
        let available_usdc = dec!(10);

        let max_by_capital = (available_usdc / bundle_cost)
            .floor()
            .to_u64()
            .unwrap_or(0);

        // -------------------------------------------------
        // 🔒 LIQUIDITY PLACEHOLDER (SAFE)
        // (REAL DEPTH COMES LATER)
        // -------------------------------------------------
        let eth_liquidity: u64 = 1_000;
        let btc_liquidity: u64 = 1_000;

        let max_by_liquidity = std::cmp::min(eth_liquidity, btc_liquidity);

        // -------------------------------------------------
        // FINAL SHARES (INTEGER ONLY)
        // -------------------------------------------------
        let shares = std::cmp::min(max_by_capital, max_by_liquidity);

        if shares == 0 {
            return None;
        }

        let shares_dec = Decimal::from(shares);

        let total_cost = bundle_cost * shares_dec;
        let expected_profit = profit_per_bundle * shares_dec;

        Some(ArbitrageOpportunity {
            eth_condition_id: eth_condition_id.to_string(),
            btc_condition_id: btc_condition_id.to_string(),

            eth_up_token_id: eth_token.token_id.clone(),
            btc_down_token_id: btc_token.token_id.clone(),

            eth_up_price: eth_price,
            btc_down_price: btc_price,

            total_cost,
            expected_profit,
        })
    }
}
