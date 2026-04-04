use anyhow::{Context, Result};
use alloy::primitives::{Address, U256};
use alloy::signers::local::PrivateKeySigner;
use alloy::signers::Signer;
use alloy::sol_types::{sol, Eip712Domain};
use std::str::FromStr;
use tracing::info;
use zeroize::Zeroizing;

sol! {
    #[derive(Debug)]
    struct Order {
        uint256 salt;
        address maker;
        address signer;
        address taker;
        uint256 tokenId;
        uint256 makerAmount;
        uint256 takerAmount;
        uint256 expiration;
        uint256 nonce;
        uint256 feeRateBps;
        uint8 side;
        uint8 signingScheme;
    }
}

/// Polymarket CLOB order signer
#[derive(Clone)]
pub struct PolymarketSigner {
    wallet: PrivateKeySigner,
    chain_id: u64,
    verifying_contract: Address,
    domain: Eip712Domain,
}

impl PolymarketSigner {
    /// Create from raw private key bytes
    pub fn new(private_key_bytes: &[u8], chain_id: u64, verifying_contract: Address) -> Result<Self> {
        let wallet = PrivateKeySigner::from_slice(private_key_bytes)
            .context("Failed to create wallet from private key")?
            .with_chain_id(Some(chain_id));

        // C-5 FIX: Polymarket CTF Exchange expects this specific salt
        let salt_bytes = hex::decode("251543d4af9c5b206ce59ec472b535d94726cd55b6bd05eb0ebdc86dfcbac0e3").unwrap_or_default();
        let salt = alloy::primitives::B256::from_slice(&salt_bytes);

        let domain = Eip712Domain::new(
            Some("Polymarket CTF Exchange".into()),
            Some("1".into()),
            Some(U256::from(chain_id)),
            Some(verifying_contract),
            Some(salt),
        );

        info!(address = %wallet.address(), chain_id, "Polymarket signer initialized");
        Ok(Self { wallet, chain_id, verifying_contract, domain })
    }

    /// Create from hex-encoded private key string
    pub fn from_hex(hex_key: &str, chain_id: u64, verifying_contract: Address) -> Result<Self> {
        let key = hex_key.strip_prefix("0x").unwrap_or(hex_key);
        // Wrap decoded bytes in Zeroizing so they are cleared from memory on drop
        let bytes = Zeroizing::new(hex::decode(key).context("Invalid hex private key")?);
        Self::new(&bytes, chain_id, verifying_contract)
    }

    /// Create with the default Polymarket CTF Exchange contract on Polygon
    pub fn from_hex_default(hex_key: &str, chain_id: u64) -> Result<Self> {
        let verifying_contract = Address::from_str("0x4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E")
            .context("Invalid default verifying contract address")?;
        Self::from_hex(hex_key, chain_id, verifying_contract)
    }

    pub fn address(&self) -> Address {
        self.wallet.address()
    }

    pub fn verifying_contract(&self) -> Address {
        self.verifying_contract
    }

    /// Sign a Polymarket CLOB order
    /// Returns the signature as a hex string
    pub async fn sign_order(
        &self,
        salt: U256,
        maker: Address,
        signer: Address,
        taker: Address,
        token_id: U256,
        maker_amount: U256,
        taker_amount: U256,
        expiration: U256,
        nonce: U256,
        fee_rate_bps: U256,
        side: u8,
        signing_scheme: u8,
    ) -> Result<String> {
        let order = Order {
            salt,
            maker,
            signer,
            taker,
            tokenId: token_id,
            makerAmount: maker_amount,
            takerAmount: taker_amount,
            expiration,
            nonce,
            feeRateBps: fee_rate_bps,
            side,
            signingScheme: signing_scheme,
        };

        // CRITICAL FIX: Alloy typed data signing is asynchronous. 
        // We use `sign_typed_data` and `.await` it.
        let signature = self.wallet
            .sign_typed_data(&order, &self.domain)
            .await
            .context("Failed to sign EIP-712 order")?;

        Ok(format!("0x{}", hex::encode(signature.as_bytes())))
    }
}