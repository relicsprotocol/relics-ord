#![allow(clippy::eq_op)]
#![allow(clippy::no_effect)]
#![allow(clippy::identity_op)]
use super::*;

#[derive(Default, Serialize, Deserialize, Debug, PartialEq, Copy, Clone, Eq)]
pub struct Enshrining {
  /// potential mint boosts
  pub boost_terms: Option<BoostTerms>,
  /// trading fee in bps (10_000 = 100%)
  pub fee: Option<u16>,
  /// for free relics only, creator can sponsor base token lp
  pub subsidy: Option<u128>,
  /// symbol attached to this Relic
  pub symbol: Option<char>,
  /// mint parameters
  pub mint_terms: Option<MintTerms>,
}

#[derive(Default, Serialize, Deserialize, Debug, PartialEq, Copy, Clone, Eq)]
pub struct MultiMint {
  /// Number of mints to perform (always positive).
  pub count: u8,
  /// When minting, the maximum base token to spend; when unminting, the minimum base token to receive.
  pub base_limit: u128,
  /// True if this operation is an unmint (i.e. a revert), false for a mint.
  pub is_unmint: bool,
  /// The Relic ID to mint or unmint.
  pub relic: RelicId,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Copy, Clone, Eq)]
#[serde(untagged)]
pub enum PriceModel {
  // Legacy: a fixed price as a number.
  Fixed(u128),
  // New: formula pricing.
  Formula { a: u128, b: u128, c: u128 },
}

impl PriceModel {
  pub fn compute_price(&self, x: u128) -> Option<u128> {
    match *self {
      PriceModel::Fixed(price) => Some(price),
      PriceModel::Formula { a, b, c } => {
        let denom = c.checked_add(x)?;
        (denom > 0).then(|| a.saturating_sub(b / denom))
      }
    }
  }

  /// Computes the total price for `count` mints starting at mint index `start`.
  pub fn compute_total_price(&self, start: u128, count: u8) -> Option<u128> {
    match *self {
      PriceModel::Fixed(price) => price.checked_mul(count as u128),
      PriceModel::Formula { .. } => {
        let mut total = 0u128;
        for i in 0..count {
          let x = start.checked_add(i as u128)?;
          if let Some(computed_price) = self.compute_price(x) {
            total = total.checked_add(computed_price)?;
          } else {
            return None;
          }
        }
        Some(total)
      }
    }
  }
}

/// Allows minting of tokens for a fixed price until the total supply was minted.
/// Afterward, the liquidity pool is immediately opened with the total RELIC collected during minting and the Relics seed supply.
/// If the Relic never mints out, no pool is created and the collected RELIC are locked.
#[derive(Default, Serialize, Deserialize, Debug, PartialEq, Copy, Clone, Eq)]
pub struct MintTerms {
  /// amount of quote tokens minted per mint
  pub amount: Option<u128>,
  /// Maximum number of mints allowed in one block
  pub block_cap: Option<u32>,
  /// maximum number of mints allowed
  /// if mint is boosted, this is only a soft cap
  pub cap: Option<u128>,
  /// Only if set, tokens can be unminted (until max_unmints reached)
  pub max_unmints: Option<u32>,
  /// note: must be set, except for RELIC, which does not have a price
  pub price: Option<PriceModel>,
  /// initial supply of quote tokens when the liquidity pool is created
  /// the typical case would be to set this to amount*cap
  pub seed: Option<u128>,
  /// Maximum number of mints allowed in one transaction
  pub tx_cap: Option<u8>,
}

/// If set give people the chance to get boosts (multipliers) on their mints
#[derive(Default, Serialize, Deserialize, Debug, PartialEq, Copy, Clone, Eq)]
pub struct BoostTerms {
  // chance to get a rare mint in ppm
  pub rare_chance: Option<u32>,
  // e.g. if set to 10 -> rare mint = min. 1x mint amount, max 10x mint amount
  pub rare_multiplier_cap: Option<u16>,
  // chance to get an ultra rare mint in ppm
  pub ultra_rare_chance: Option<u32>,
  // e.g. if set to 20 and rare mint set to 10 -> min 10x mint amount, max 20x mint amount
  pub ultra_rare_multiplier_cap: Option<u16>,
}

impl BoostTerms {
  fn validate(&self, amount: Option<u128>) -> Result<(), RelicFlaw> {
    let (rc, rm) = self
      .rare_tuple()
      .ok_or(RelicFlaw::InvalidEnshriningBoostInvalidRareBoost)?;
    let (urc, urm) = self
      .ultra_tuple()
      .ok_or(RelicFlaw::InvalidEnshriningBoostInvalidUltraRareBoost)?;

    ensure!(
      rc <= 999_999,
      RelicFlaw::InvalidEnshriningBoostInvalidRareChance
    );
    ensure!(
      urc <= 999_999,
      RelicFlaw::InvalidEnshriningBoostInvalidUltraRareChance
    );

    ensure!(urc < rc, RelicFlaw::InvalidEnshriningBoostChanceOrder);
    ensure!(urm > rm, RelicFlaw::InvalidEnshriningBoostMultiplierOrder);

    if let Some(a) = amount {
      ensure!(
        a.checked_mul(rm as u128).is_some(),
        RelicFlaw::InvalidEnshriningBoostRareAmountOverflow
      );
      ensure!(
        a.checked_mul(urm as u128).is_some(),
        RelicFlaw::InvalidEnshriningBoostUltraRareAmountOverflow
      );
    }
    Ok(())
  }

  // helpers returning Option<(chance, mult)>
  fn rare_tuple(&self) -> Option<(u32, u16)> {
    Some((self.rare_chance?, self.rare_multiplier_cap?))
  }
  fn ultra_tuple(&self) -> Option<(u32, u16)> {
    Some((self.ultra_rare_chance?, self.ultra_rare_multiplier_cap?))
  }
}

impl MintTerms {
  // Compute price using the current mint count (x)
  pub fn compute_price(&self, x: u128) -> Option<u128> {
    self.price.and_then(|p| p.compute_price(x))
  }

  /// Computes the total price for `count` mints starting at mint index `start`.
  pub fn compute_total_price(&self, start: u128, count: u8) -> Option<u128> {
    self.price.and_then(|p| p.compute_total_price(start, count))
  }

  fn validate(&self) -> Result<(), RelicFlaw> {
    let cap = self
      .cap
      .ok_or(RelicFlaw::InvalidEnshriningTermsMissingOrZeroCap)?;
    ensure!(cap != 0, RelicFlaw::InvalidEnshriningTermsMissingOrZeroCap);

    if let Some(amount) = self.amount {
      ensure!(
        cap.checked_mul(amount).is_some(),
        RelicFlaw::InvalidEnshriningTermsAmountCapOverflow
      );
    }

    match self.price {
      Some(PriceModel::Fixed(p)) => ensure!(
        cap.checked_mul(p).is_some(),
        RelicFlaw::InvalidEnshriningTermsFixedPriceCapOverflow
      ),
      Some(PriceModel::Formula { a, b, c }) => ensure!(
        c > 0 && (b / c) <= a && cap <= 1_000_000,
        RelicFlaw::InvalidEnshriningTermsInvalidPriceFormula
      ),
      None => return Err(RelicFlaw::InvalidEnshriningTermsMissingPrice),
    }

    if let Some(block_cap) = self.block_cap {
      ensure!(
        cap >= block_cap as u128,
        RelicFlaw::InvalidEnshriningTermsInvalidCapHierarchy
      );
      if let Some(tx_cap) = self.tx_cap {
        ensure!(
          block_cap >= tx_cap as u32,
          RelicFlaw::InvalidEnshriningTermsInvalidCapHierarchy
        );
      }
    }
    Ok(())
  }
}

impl Enshrining {
  /// All Relics come with the same divisibility
  pub const DIVISIBILITY: u8 = 8;
  pub const MAX_SPACERS: u32 = 0b00000111_11111111_11111111_11111111;

  pub fn max_supply(&self) -> Option<u128> {
    let amount = self
      .mint_terms
      .and_then(|terms| terms.amount)
      .unwrap_or_default();
    let cap = self
      .mint_terms
      .and_then(|terms| terms.cap)
      .unwrap_or_default();
    let seed = self
      .mint_terms
      .and_then(|terms| terms.seed)
      .unwrap_or_default();

    // If ultra_rare_multiplier_cap is not set, use rare_multiplier_cap; if that's also not set, use 1.
    let max_boost = self
      .boost_terms
      .map(|b| {
        b.ultra_rare_multiplier_cap
          .unwrap_or(b.rare_multiplier_cap.unwrap_or(1))
      })
      .unwrap_or(1);

    // assume the highest supply possible
    seed.checked_add(cap.checked_mul(amount)?.checked_mul(max_boost.into())?)
  }

  pub fn validate(&self) -> Result<(), RelicFlaw> {
    let terms = self
      .mint_terms
      .as_ref()
      .ok_or(RelicFlaw::InvalidEnshriningTermsMissingOrZeroCap)?;
    terms.validate()?;

    if let Some(bt) = &self.boost_terms {
      bt.validate(terms.amount)?;
    }

    self
      .max_supply()
      .ok_or(RelicFlaw::InvalidEnshriningMaxSupplyCalculation)?;

    // Subsidy ↔ price rules
    match (self.subsidy, terms.price) {
      (Some(s), Some(PriceModel::Fixed(p))) => {
        ensure!(s > 0 && p == 0, RelicFlaw::InvalidEnshriningSubsidyRules)
      }
      (Some(_), _) => return Err(RelicFlaw::InvalidEnshriningSubsidyRules),
      (None, Some(PriceModel::Fixed(0))) => return Err(RelicFlaw::InvalidEnshriningSubsidyRules),
      _ => {}
    }

    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;

  #[test]
  fn test_price_model_fixed() {
    let fixed_price = PriceModel::Fixed(1000);
    assert_eq!(fixed_price.compute_price(0), Some(1000));
    assert_eq!(fixed_price.compute_price(1), Some(1000));
    assert_eq!(fixed_price.compute_price(100), Some(1000));
  }

  #[test]
  fn test_price_model_formula() {
    // price = a - b / (mints + c)
    let mut formula_price = PriceModel::Formula { a: 10, b: 5, c: 2 };
    assert_eq!(formula_price.compute_price(0), Some(10 - 5 / (0 + 2)));
    assert_eq!(formula_price.compute_price(1), Some(10 - 5 / (1 + 2)));
    assert_eq!(formula_price.compute_price(2), Some(10 - 5 / (2 + 2)));
    assert_eq!(formula_price.compute_price(3), Some(10 - 5 / (3 + 2)));
    assert_eq!(formula_price.compute_price(8), Some(10 - 5 / (8 + 2)));
    assert_eq!(formula_price.compute_price(47), Some(10 - 5 / (47 + 2)));
    assert_eq!(formula_price.compute_price(999), Some(10 - 5 / (999 + 2)));

    formula_price = PriceModel::Formula {
      a: 1000,
      b: 500,
      c: 1,
    };

    assert_eq!(formula_price.compute_price(0), Some(1000 - 500 / (0 + 1)));
    assert_eq!(formula_price.compute_price(1), Some(1000 - 500 / (1 + 1)));
    assert_eq!(formula_price.compute_price(10), Some(1000 - 500 / (10 + 1)));
    assert_eq!(
      formula_price.compute_price(500),
      Some(1000 - 500 / (500 + 1))
    );
    assert_eq!(
      formula_price.compute_price(50_000),
      Some(1000 - 500 / (50_000 + 1))
    );
  }

  #[test]
  fn test_price_model_formula_multi_mint() {
    let formula = PriceModel::Formula {
      a: 500,
      b: 1000,
      c: 1,
    };

    assert_eq!(formula.compute_total_price(0, 3), Some(167));
    assert_eq!(formula.compute_total_price(3, 2), Some(550));
    assert_eq!(formula.compute_total_price(10, 5), Some(2114));
    assert_eq!(formula.compute_total_price(100, 2), Some(982));
    assert_eq!(formula.compute_total_price(50000, 2), Some(1000));
  }

  #[test]
  fn test_formula_with_starting_price_exact() {
    let formula = PriceModel::Formula {
      a: 10000,
      b: 50000,
      c: 10,
    };

    assert_eq!(formula.compute_price(0), Some(5000));
    assert_eq!(formula.compute_price(1), Some(5455));
    assert_eq!(formula.compute_price(2), Some(5834));
  }

  #[test]
  fn test_zero_division_protection() {
    let div_zero_formula = PriceModel::Formula {
      a: 1000,
      b: 500,
      c: 0,
    };
    assert_eq!(div_zero_formula.compute_price(0), None);
    assert_eq!(div_zero_formula.compute_price(1), Some(500));
  }
}
