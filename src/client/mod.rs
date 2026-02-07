use crate::domain::*;
use anyhow::{Context, Result};
use base64::{engine::general_purpose, Engine as _};
use hmac::{Hmac, Mac};
use reqwest::Client;
use rust_decimal::Decimal;
use serde_json::Value;
use sha2::Sha256;
use std::str::FromStr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct PolymarketClient {
    client: Client,
    pub gamma_url: String,
    pub clob_url: String,

    pub api_key: String,
    api_secret: String,
    api_passphrase: String,

    pub read_only: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SignedOrderPayload {
    pub order: OrderRequest,
    pub signature: String,
    pub address: String,
}

impl PolymarketClient {
    // ==================================================
    // CONSTRUCTOR
    // ==================================================
    pub fn new(
        gamma_url: String,
        clob_url: String,
        api_key: String,
        api_secret: String,
        api_passphrase: String,
        read_only: bool,
    ) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            gamma_url,
            clob_url,
            api_key,
            api_secret,
            api_passphrase,
            read_only,
        }
    }

    // ==================================================
    // USDC BALANCE (REAL – API KEY SCOPE)
    // ==================================================
    pub async fn get_usdc_balance(&self) -> Result<Decimal> {
    let url = format!("{}/balances/me", self.clob_url);

    let response = self
        .client
        .get(&url)
        .header("Authorization", format!("Bearer {}", self.api_key))
        .send()
        .await?;

    if !response.status().is_success() {
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("Balance fetch failed: {}", text);
    }

    let balances: Vec<crate::domain::Balance> = response.json().await?;

    for b in balances {
        if b.asset.eq_ignore_ascii_case("USDC") {
            return Ok(b.balance);
        }
    }

    Ok(Decimal::ZERO)
}


    // ==================================================
    // HMAC SIGNING
    // ==================================================
    fn sign_request(
        &self,
        method: &str,
        path: &str,
        body: &str,
        timestamp: &str,
    ) -> String {
        let payload = format!(
            "{}{}{}{}",
            timestamp,
            method.to_uppercase(),
            path,
            body
        );

        let mut mac =
            HmacSha256::new_from_slice(self.api_secret.as_bytes())
                .expect("HMAC init failed");

        mac.update(payload.as_bytes());
        general_purpose::STANDARD.encode(mac.finalize().into_bytes())
    }

    // ==================================================
    // MARKETS
    // ==================================================
    pub async fn get_market_by_slug(&self, slug: &str) -> Result<Market> {
        let url = format!("{}/events/slug/{}", self.gamma_url, slug);
        let response = self.client.get(&url).send().await?;
        let json: Value = response.json().await?;

        json["markets"]
            .as_array()
            .and_then(|m| m.first())
            .map(|m| serde_json::from_value(m.clone()).unwrap())
            .context("Market not found")
    }

    pub async fn get_market(
        &self,
        condition_id: &str,
    ) -> Result<MarketDetails> {
        let url = format!("{}/markets/{}", self.clob_url, condition_id);
        Ok(self.client.get(&url).send().await?.json().await?)
    }

    // ==================================================
    // PRICE
    // ==================================================
    pub async fn get_price(
        &self,
        token_id: &str,
        side: &str,
    ) -> Result<Decimal> {
        let url = format!("{}/price", self.clob_url);
        let params = [("token_id", token_id), ("side", side)];

        let json: Value = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await?
            .json()
            .await?;

        let price = json["price"]
            .as_str()
            .context("Missing price")?;

        Ok(Decimal::from_str(price)?)
    }

    // ==================================================
    // PLACE SIGNED ORDER (REAL TRADING)
    // ==================================================
    pub async fn place_signed_order(
        &self,
        payload: &SignedOrderPayload,
    ) -> Result<OrderResponse> {
        if self.read_only {
            anyhow::bail!("READ-ONLY MODE ENABLED");
        }

        let path = "/orders";
        let url = format!("{}{}", self.clob_url, path);
        let body = serde_json::to_string(payload)?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs()
            .to_string();

        let signature =
            self.sign_request("POST", path, &body, &timestamp);

        let response = self
            .client
            .post(&url)
            .header("POLY-API-KEY", &self.api_key)
            .header("POLY-API-SIGNATURE", signature)
            .header("POLY-API-TIMESTAMP", &timestamp)
            .header("POLY-API-PASSPHRASE", &self.api_passphrase)
            .json(payload)
            .send()
            .await?;

        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("Order rejected: {}", text);
        }

        Ok(response.json().await?)
    }
}
