//! HTTP client for Uniswap REST APIs (Trade, LP, positions).
//!
//! Real client calls the base URL configured via `UNISWAP_API_URL`. A mock
//! client returns hardcoded deterministic data, useful for local dev when
//! the API is unreachable or rate-limited.
//!
//! The real `UniswapClient::get_pool_state` is wired into the orchestrator's
//! epoch loop; `MockUniswapClient` is retained as cold-start / hard-failure
//! fallback for local dev without network access.

use anyhow::Context;
use prism_types::ProtocolState;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct UniswapClient {
    base_url: String,
    http: reqwest::Client,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SwapQuote {
    pub amount_out: u128,
    pub price_impact_bps: u16,
    pub gas_estimate: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LpPosition {
    pub token_id: u64,
    pub pool: String,
    pub liquidity: u128,
    pub tick_lower: i32,
    pub tick_upper: i32,
}

// ---------------------------------------------------------------------------
// Upstream response shapes (only fields we consume).
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct PoolResponse {
    #[serde(rename = "sqrtPriceX96")]
    sqrt_price_x96: StringU128,
    liquidity: StringU128,
    tick: i32,
    #[serde(rename = "feeTier")]
    fee_tier: u32,
    token0: TokenSide,
    token1: TokenSide,
    #[serde(default)]
    volatility_bps: Option<u32>,
}

#[derive(Deserialize)]
struct TokenSide {
    reserve: StringU128,
}

/// u128 serialized as a decimal string — the Uniswap API convention.
#[derive(Deserialize)]
#[serde(transparent)]
struct StringU128(String);

impl StringU128 {
    fn into_u128(self) -> anyhow::Result<u128> {
        self.0.parse::<u128>().context("decoding u128")
    }
}

#[derive(Deserialize)]
struct QuoteResponse {
    #[serde(rename = "amountOut")]
    amount_out: StringU128,
    #[serde(rename = "priceImpactBps")]
    price_impact_bps: u16,
    #[serde(rename = "gasEstimate")]
    gas_estimate: u64,
}

#[derive(Deserialize)]
struct PositionsResponse {
    positions: Vec<LpPositionWire>,
}

#[derive(Deserialize)]
struct LpPositionWire {
    #[serde(rename = "tokenId")]
    token_id: u64,
    pool: String,
    liquidity: StringU128,
    #[serde(rename = "tickLower")]
    tick_lower: i32,
    #[serde(rename = "tickUpper")]
    tick_upper: i32,
}

// ---------------------------------------------------------------------------

impl UniswapClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
        }
    }

    pub async fn get_pool_state(&self, pool_address: &str) -> anyhow::Result<ProtocolState> {
        let url = format!("{}/v1/pools/{}", self.base_url, pool_address);
        let resp: PoolResponse = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {}", url))?
            .error_for_status()?
            .json()
            .await?;

        Ok(ProtocolState {
            pool_address: parse_addr(pool_address)?,
            sqrt_price_x96: resp.sqrt_price_x96.into_u128()?,
            liquidity: resp.liquidity.into_u128()?,
            tick: resp.tick,
            fee_tier: resp.fee_tier,
            token0_reserve: resp.token0.reserve.into_u128()?,
            token1_reserve: resp.token1.reserve.into_u128()?,
            volatility_30d_bps: resp.volatility_bps.unwrap_or(1_500),
        })
    }

    pub async fn get_swap_quote(
        &self,
        token_in: &str,
        token_out: &str,
        amount_in: u128,
    ) -> anyhow::Result<SwapQuote> {
        let url = format!(
            "{}/v1/quote?tokenIn={}&tokenOut={}&amount={}",
            self.base_url, token_in, token_out, amount_in
        );
        let resp: QuoteResponse = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {}", url))?
            .error_for_status()?
            .json()
            .await?;

        Ok(SwapQuote {
            amount_out: resp.amount_out.into_u128()?,
            price_impact_bps: resp.price_impact_bps,
            gas_estimate: resp.gas_estimate,
        })
    }

    pub async fn get_lp_positions(&self, owner: &str) -> anyhow::Result<Vec<LpPosition>> {
        let url = format!("{}/v1/positions/{}", self.base_url, owner);
        let resp: PositionsResponse = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {}", url))?
            .error_for_status()?
            .json()
            .await?;

        let mut out = Vec::with_capacity(resp.positions.len());
        for p in resp.positions {
            out.push(LpPosition {
                token_id: p.token_id,
                pool: p.pool,
                liquidity: p.liquidity.into_u128()?,
                tick_lower: p.tick_lower,
                tick_upper: p.tick_upper,
            });
        }
        Ok(out)
    }
}

fn parse_addr(s: &str) -> anyhow::Result<[u8; 20]> {
    let s = s.trim_start_matches("0x");
    let bytes = hex::decode(s).context("decoding pool address hex")?;
    anyhow::ensure!(bytes.len() == 20, "pool address must be 20 bytes");
    let mut out = [0u8; 20];
    out.copy_from_slice(&bytes);
    Ok(out)
}

// ---------------------------------------------------------------------------
// Mock client — used in dev and tests.
// ---------------------------------------------------------------------------

pub struct MockUniswapClient;

impl MockUniswapClient {
    pub fn get_pool_state(&self, pool_address: &str) -> ProtocolState {
        let addr = parse_addr(pool_address).unwrap_or([0xDD; 20]);
        ProtocolState {
            pool_address: addr,
            sqrt_price_x96: 4_339_505_179_874_584_694_521u128, // ~price 3000 ETH/USDC
            liquidity: 1_500_000_000_000_000u128,
            tick: 200_000,
            fee_tier: 3_000,
            token0_reserve: 10_000_000_000_000_000_000_000u128,
            token1_reserve: 30_000_000_000_000u128,
            volatility_30d_bps: 1_500,
        }
    }

    pub fn get_swap_quote(&self, _token_in: &str, _token_out: &str, amount_in: u128) -> SwapQuote {
        SwapQuote {
            amount_out: amount_in.saturating_mul(997) / 1000,
            price_impact_bps: 10,
            gas_estimate: 150_000,
        }
    }

    pub fn get_lp_positions(&self, _owner: &str) -> Vec<LpPosition> {
        vec![LpPosition {
            token_id: 1,
            pool: "0x8ad599c3A0ff1De082011EFDDc58f1908eb6e6D8".into(),
            liquidity: 1_000_000_000_000_000u128,
            tick_lower: 190_000,
            tick_upper: 210_000,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_pool_state_is_sane() {
        let c = MockUniswapClient;
        let s = c.get_pool_state("0x8ad599c3A0ff1De082011EFDDc58f1908eb6e6D8");
        assert_eq!(s.fee_tier, 3_000);
        assert!(s.liquidity > 0);
    }

    #[test]
    fn mock_swap_quote_applies_03_fee() {
        let c = MockUniswapClient;
        let q = c.get_swap_quote("0xA", "0xB", 1_000_000);
        assert_eq!(q.amount_out, 997_000);
    }
}
