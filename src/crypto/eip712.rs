use anyhow::{Context, Result};
use ethers::core::types::{Address, U256};
use ethers::signers::{LocalWallet, Signer};
use std::str::FromStr;
use tracing::info;
use zeroize::Zeroizing;

/// Polymarket CLOB order signer
#[derive(Clone)]
pub struct PolymarketSigner {
    wallet: LocalWallet,
    chain_id: u64,
    verifying_contract: Address,
    domain_separator: [u8; 32],
}

impl PolymarketSigner {
    /// Create from raw private key bytes
    pub fn new(private_key_bytes: &[u8], chain_id: u64, verifying_contract: Address) -> Result<Self> {
        let wallet = LocalWallet::from_bytes(private_key_bytes)
            .context("Failed to create wallet from private key")?
            .with_chain_id(chain_id);

        let domain_separator = Self::build_domain_separator(chain_id, verifying_contract);

        info!(address = %wallet.address(), chain_id, "Polymarket signer initialized");
        Ok(Self { wallet, chain_id, verifying_contract, domain_separator })
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
        let order_hash = self.compute_order_hash(
            salt, maker, signer, taker, token_id,
            maker_amount, taker_amount, expiration,
            nonce, fee_rate_bps, side, signing_scheme,
        )?;

        let signature = self.wallet
            .sign_hash(ethers::types::H256::from(order_hash))
            .context("Failed to sign order hash")?;

        Ok(format!("0x{}", signature))
    }

    fn compute_order_hash(
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
    ) -> Result<[u8; 32]> {
        use ethers::abi::{encode, Token};
        use ethers::utils::keccak256;

        // ORDER_TYPEHASH
        let order_typehash = keccak256(
            b"Order(uint256 salt,address maker,address signer,address taker,uint256 tokenId,uint256 makerAmount,uint256 takerAmount,uint256 expiration,uint256 nonce,uint256 feeRateBps,uint8 side,uint8 signingScheme)"
        );

        let encoded = encode(&[
            Token::FixedBytes(order_typehash.to_vec()),
            Token::Uint(salt),
            Token::Address(maker),
            Token::Address(signer),
            Token::Address(taker),
            Token::Uint(token_id),
            Token::Uint(maker_amount),
            Token::Uint(taker_amount),
            Token::Uint(expiration),
            Token::Uint(nonce),
            Token::Uint(fee_rate_bps),
            Token::Uint(U256::from(side)),
            Token::Uint(U256::from(signing_scheme)),
        ]);

        let struct_hash = keccak256(&encoded);

        // Use the cached domain separator (computed once in the constructor)
        let domain_separator = &self.domain_separator;

        // EIP-712 hash: keccak256("\x19\x01" || domainSeparator || structHash)
        let mut eip712_msg = Vec::with_capacity(66);
        eip712_msg.extend_from_slice(&[0x19, 0x01]);
        eip712_msg.extend_from_slice(domain_separator);
        eip712_msg.extend_from_slice(&struct_hash);

        Ok(keccak256(&eip712_msg))
    }

    fn build_domain_separator(chain_id: u64, verifying_contract: Address) -> [u8; 32] {
        use ethers::abi::{encode, Token};
        use ethers::utils::keccak256;

        let domain_typehash = keccak256(
            b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)"
        );

        let encoded = encode(&[
            Token::FixedBytes(domain_typehash.to_vec()),
            Token::FixedBytes(keccak256(b"Polymarket CTF Exchange").to_vec()),
            Token::FixedBytes(keccak256(b"1").to_vec()),
            Token::Uint(U256::from(chain_id)),
            Token::Address(verifying_contract),
        ]);

        keccak256(&encoded)
    }
}
