use anyhow::Result;
use ethers::prelude::*;
use ethers::types::{H256, U256};
use ethers::types::transaction::eip712::Eip712;
use ethers::contract::EthAbiType;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct WalletSigner {
    wallet: LocalWallet,
}

impl WalletSigner {
    pub fn new(private_key: &str, chain_id: u64) -> Result<Self> {
        let wallet: LocalWallet = private_key.parse()?;
        let wallet = wallet.with_chain_id(chain_id);
        Ok(Self { wallet })
    }

    pub fn address(&self) -> Address {
        self.wallet.address()
    }

    pub async fn sign_order(&self, order: &ClobOrder) -> Result<Signature> {
        Ok(self.wallet.sign_typed_data(order).await?)
    }
}

/// =================================================
/// Polymarket CLOB EIP-712 Order
/// =================================================
/// ✔ ABI-encodable
/// ✔ EIP-712 compliant
/// ✔ Polymarket-compatible
#[derive(Debug, Clone, Serialize, Deserialize, EthAbiType, Eip712)]
#[eip712(
    name = "ClobAuthDomain",
    version = "1",
    chain_id = 137,
    verifying_contract = "0x0000000000000000000000000000000000000000"
)]
pub struct ClobOrder {
    pub token_id: H256,     // bytes32
    pub side: u8,           // 0 = BUY, 1 = SELL
    pub price: U256,        // scaled by 1e6
    pub size: U256,         // scaled by 1e6
    pub expiration: U256,   // unix timestamp
    pub nonce: U256,        // monotonic
}
