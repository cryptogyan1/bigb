use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct BookLevel {
    price: f64,
    size: f64,
}

#[derive(Debug, Deserialize)]
struct OrderBook {
    asks: Vec<BookLevel>,
    bids: Vec<BookLevel>,
}

#[derive(Debug, Deserialize)]
struct MidpointResp {
    midpoint: f64,
}

#[tokio::main]
async fn main() -> Result<()> {
    // 🔴 PUT ANY REAL TOKEN_ID HERE (YES or NO)
    let token_id = std::env::args()
        .nth(1)
        .expect("Usage: cargo run --bin verify_ask <TOKEN_ID>");

    let clob = std::env::var("POLYMARKET_CLOB_REST")
        .unwrap_or_else(|_| "https://clob.polymarket.com".to_string());

    let client = reqwest::Client::new();

    // ---------------------------
    // 1️⃣ Fetch real orderbook
    // ---------------------------
    let ob_url = format!("{}/orderbook/{}", clob, token_id);
    let ob: OrderBook = client.get(&ob_url).send().await?.json().await?;

    let best_ask = ob.asks.first();
    let best_bid = ob.bids.first();

    // ---------------------------
    // 2️⃣ Fetch UI midpoint
    // ---------------------------
    let mid_url = format!("{}/midpoint?token_id={}", clob, token_id);
    let midpoint: Option<MidpointResp> =
        client.get(&mid_url).send().await?.json().await.ok();

    // ---------------------------
    // 3️⃣ Print proof
    // ---------------------------
    println!("\n=== POLYMARKET PRICE VERIFICATION ===");
    println!("TOKEN_ID: {}\n", token_id);

    match best_ask {
        Some(a) => println!("BEST ASK  (buy)  → price={} size={}", a.price, a.size),
        None => println!("BEST ASK  → none"),
    }

    match best_bid {
        Some(b) => println!("BEST BID  (sell) → price={} size={}", b.price, b.size),
        None => println!("BEST BID  → none"),
    }

    match midpoint {
        Some(m) => println!("MIDPOINT (UI)    → price={}", m.midpoint),
        None => println!("MIDPOINT         → unavailable"),
    }

    println!("\nNOTE:");
    println!("• BEST ASK is the real executable price");
    println!("• MIDPOINT is UI-only and NOT tradable");
    println!("=====================================\n");

    Ok(())
}
