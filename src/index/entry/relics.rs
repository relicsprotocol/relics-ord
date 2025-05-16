use {super::*, bitcoin::ScriptHash};

impl Entry for Relic {
  type Value = u128;

  fn load(value: Self::Value) -> Self {
    Self(value)
  }

  fn store(self) -> Self::Value {
    self.0
  }
}

pub type SpacedRelicValue = (u128, u32);

impl Entry for SpacedRelic {
  type Value = SpacedRelicValue;

  fn load(value: Self::Value) -> Self {
    SpacedRelic::new(Relic(value.0), value.1)
  }

  fn store(self) -> Self::Value {
    (self.relic.0, self.spacers)
  }
}

#[derive(Debug, Hash, Eq, PartialEq, PartialOrd, Ord, Copy, Clone, Serialize, Deserialize)]
pub struct RelicOwner(pub ScriptHash);

impl Default for RelicOwner {
  fn default() -> Self {
    Self(ScriptHash::all_zeros())
  }
}

pub type RelicOwnerValue = [u8; 20];

impl Entry for RelicOwner {
  type Value = RelicOwnerValue;

  fn load(value: Self::Value) -> Self {
    Self(ScriptHash::from_byte_array(value))
  }

  fn store(self) -> Self::Value {
    self.0.to_byte_array()
  }
}

#[derive(Debug, Default, PartialEq, Copy, Clone, Serialize, Deserialize)]
pub struct RelicState {
  pub burned: u128,
  pub mints: u128,
  pub unmints: u128,
}

pub type RelicStateValue = (u128, u128, u128);

impl Entry for RelicState {
  type Value = RelicStateValue;

  fn load((burned, mints, unmints): Self::Value) -> Self {
    Self {
      burned,
      mints,
      unmints,
    }
  }

  fn store(self) -> Self::Value {
    (self.burned, self.mints, self.unmints)
  }
}

#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
pub struct RelicEntry {
  pub block: u64,
  pub enshrining: Txid,
  pub fee: u16,
  pub number: u64,
  pub spaced_relic: SpacedRelic,
  pub symbol: Option<char>,
  pub owner_sequence_number: Option<u32>,
  pub boost_terms: Option<BoostTerms>,
  pub mint_terms: Option<MintTerms>,
  pub state: RelicState,
  pub pool: Option<Pool>,
  pub timestamp: u64,
}

impl RelicEntry {
  pub fn mintable(
    &self,
    base_balance: u128,
    num_mints: u8,
    base_limit: u128,
  ) -> Result<Vec<(u128, u128)>, RelicError> {
    let terms = self.mint_terms.ok_or(RelicError::Unmintable)?;
    if self.is_free() && num_mints > 1 {
      return Err(RelicError::MaxMintPerTxExceeded(1));
    }
    if let Some(max_tx) = terms.tx_cap {
      if num_mints > max_tx {
        return Err(RelicError::MaxMintPerTxExceeded(max_tx));
      }
    }

    let cap = terms.cap.unwrap_or_default();
    let current_mints = self.state.mints;

    let remaining = cap.saturating_sub(current_mints); // mints left
    if remaining == 0 {
      return Err(RelicError::MintCap(cap)); // none left
    }
    #[allow(clippy::cast_possible_truncation)]
    let actual_mints = remaining.min(u128::from(num_mints).min(u128::from(u8::MAX))) as u8;

    let total_price = terms
      .compute_total_price(current_mints, actual_mints)
      .ok_or(RelicError::PriceComputationError)?;

    if base_limit < total_price {
      return Err(RelicError::MintBaseLimitExceeded(base_limit, total_price));
    }
    if base_balance < total_price {
      return Err(RelicError::MintInsufficientBalance(total_price));
    }

    let mut results = Vec::with_capacity(actual_mints as usize);
    for x in current_mints..current_mints + u128::from(actual_mints) {
      let price = terms
        .compute_price(x)
        .ok_or(RelicError::PriceComputationError)?;
      let amount = terms.amount.unwrap_or_default();
      results.push((amount, price));
    }
    Ok(results)
  }

  // base_min = minimum base tokens to accept
  pub fn unmintable(
    &self,
    balance: u128,
    num_mints: u8,
    base_min: u128,
  ) -> Result<Vec<(u128, u128)>, RelicError> {
    let terms = self.mint_terms.as_ref().ok_or(RelicError::Unmintable)?;
    let max_unmints = u128::from(terms.max_unmints.ok_or(RelicError::UnmintNotAllowed)?);
    if self.state.mints < u128::from(num_mints) {
      return Err(RelicError::NoMintsToUnmint);
    }
    if self.is_free() {
      return Err(RelicError::UnmintNotAllowed);
    }
    let cap = terms.cap.unwrap_or(0);
    if cap != 0 && self.state.mints == cap {
      return Err(RelicError::UnmintNotAllowed);
    }
    if self.state.unmints + u128::from(num_mints) > max_unmints {
      return Err(RelicError::UnmintNotAllowed);
    }
    let mut results = Vec::with_capacity(num_mints as usize);
    let mut total_amount: u128 = 0;
    let mut total_price: u128 = 0;
    for i in 0..num_mints {
      let mint_index = self.state.mints - 1 - u128::from(i);
      let price = match terms.price {
        Some(PriceModel::Fixed(fixed)) => fixed,
        Some(PriceModel::Formula { .. }) => terms
          .compute_price(mint_index)
          .ok_or(RelicError::PriceComputationError)?,
        None => return Err(RelicError::PriceComputationError),
      };
      let amount = terms.amount.unwrap_or_default();
      total_amount = total_amount.saturating_add(amount);
      total_price = total_price.saturating_add(price);
      results.push((amount, price));
    }
    if balance < total_amount {
      return Err(RelicError::MintInsufficientBalance(total_amount));
    }
    if total_price < base_min {
      return Err(RelicError::MintBaseLimitExceeded(base_min, total_price));
    }
    Ok(results)
  }

  pub fn swap(&self, swap: PoolSwap, balance: Option<u128>) -> Result<BalanceDiff, RelicError> {
    // fail the swap if pool does not exist (yet)
    let Some(pool) = self.pool else {
      return Err(RelicError::SwapNotAvailable);
    };

    // fail the swap if either the base supply or quote supply is 0
    if pool.base_supply == 0 || pool.quote_supply == 0 {
      return Err(RelicError::SwapNotAvailable);
    }

    // also fail if pool still has subsidy -> means not fully minted
    if pool.subsidy > 0 {
      return Err(RelicError::SwapNotAvailable);
    }

    match pool.calculate(swap) {
      Ok(diff) => {
        if let Some(balance) = balance {
          if diff.input > balance {
            return Err(RelicError::SwapInsufficientBalance(diff.input));
          }
        }
        Ok(diff)
      }
      Err(cause) => Err(RelicError::SwapFailed(cause)),
    }
  }

  /// max supply of this token: maximum amount of tokens that can be minted plus
  /// the additional amount that is created for the pool after minting is complete
  pub fn max_supply(&self) -> u128 {
    self
      .mint_terms
      .map(|terms| {
        terms.amount.unwrap_or_default() * terms.cap.unwrap_or_default()
          + terms.seed.unwrap_or_default()
      })
      .unwrap_or_default()
  }

  pub fn is_free(&self) -> bool {
    self
      .mint_terms
      .as_ref()
      .map(|terms| terms.price.is_none() || matches!(terms.price, Some(PriceModel::Fixed(0))))
      .unwrap_or(false)
  }

  /// circulating supply of tokens: either minted or swapped out of the pool minus burned
  pub fn circulating_supply(&self) -> u128 {
    let amount = self
      .mint_terms
      .and_then(|terms| terms.amount)
      .unwrap_or_default();
    let seed = self
      .mint_terms
      .and_then(|terms| terms.seed)
      .unwrap_or_default();
    let pool_quote_supply = self.pool.map(|pool| pool.quote_supply).unwrap_or(seed);
    self.state.mints * amount + seed - pool_quote_supply - self.state.burned
  }

  pub fn locked_base_supply(&self) -> u128 {
    if let Some(pool) = self.pool {
      if pool.base_supply > 0 {
        return pool.base_supply; // pool was already bootstrapped
      } else if pool.subsidy > 0 {
        return pool.subsidy; // lp is sponsored
      }
    }

    // otherwise take from the mint
    if let Some(terms) = self.mint_terms {
      match terms.price {
        Some(PriceModel::Fixed(fixed)) => self.state.mints * fixed,
        Some(PriceModel::Formula { .. }) => {
          let mut total: u128 = 0;
          for x in 0..self.state.mints {
            total = total.saturating_add(terms.compute_price(x).unwrap_or(0));
          }
          total
        }
        None => 0,
      }
    } else {
      0
    }
  }
}

type MintTermsValue = (
  Option<u128>, // amount
  Option<u32>,  // block cap
  Option<u128>, // cap
  Option<u32>,  // max unmints
  Option<u128>, // stored price:
  //   - Some(n) with n != 0 represents PriceModel::Fixed(n)
  //   - Some(0) indicates formula pricing (with a, b, c below)
  Option<u128>, // formula_a (for formula pricing)
  Option<u128>, // formula_b (for formula pricing)
  Option<u128>, // formula_c (for formula pricing)
  Option<u128>, // seed
  Option<u8>,   // tx cap
);

impl Entry for MintTerms {
  type Value = MintTermsValue;

  fn load(
    (
      amount,
      block_cap,
      cap,
      max_unmints,
      price_type,
      price_fixed_or_a,
      formula_b,
      formula_c,
      seed,
      tx_cap,
    ): Self::Value,
  ) -> Self {
    let price = match price_type {
      Some(1) => price_fixed_or_a.map(PriceModel::Fixed),
      Some(2) => {
        if let (Some(a), Some(b), Some(c)) = (price_fixed_or_a, formula_b, formula_c) {
          Some(PriceModel::Formula { a, b, c })
        } else {
          None
        }
      }
      _ => None,
    };
    Self {
      amount,
      block_cap,
      cap,
      max_unmints,
      price,
      seed,
      tx_cap,
    }
  }

  fn store(self) -> Self::Value {
    let (price_type, price_fixed_or_a, formula_b, formula_c) = match self.price {
      Some(PriceModel::Fixed(p)) => (Some(1), Some(p), None, None),
      Some(PriceModel::Formula { a, b, c }) => (Some(2), Some(a), Some(b), Some(c)),
      None => (None, None, None, None),
    };
    (
      self.amount,
      self.block_cap,
      self.cap,
      self.max_unmints,
      price_type,
      price_fixed_or_a,
      formula_b,
      formula_c,
      self.seed,
      self.tx_cap,
    )
  }
}

pub type BoostTermsValue = (
  Option<u32>, // rare_chance
  Option<u16>, // rare_multiplier
  Option<u32>, // ultra_rare_chance
  Option<u16>, // ultra_rare_multiplier
);

impl Entry for BoostTerms {
  type Value = BoostTermsValue;
  fn load(
    (rare_chance, rare_multiplier, ultra_rare_chance, ultra_rare_multiplier): Self::Value,
  ) -> Self {
    Self {
      rare_chance,
      rare_multiplier_cap: rare_multiplier,
      ultra_rare_chance,
      ultra_rare_multiplier_cap: ultra_rare_multiplier,
    }
  }
  fn store(self) -> Self::Value {
    (
      self.rare_chance,
      self.rare_multiplier_cap,
      self.ultra_rare_chance,
      self.ultra_rare_multiplier_cap,
    )
  }
}

pub type PoolValue = (u128, u128, u16, u128);

impl Entry for Pool {
  type Value = PoolValue;

  fn load((base_supply, quote_supply, fee_bps, subsidy): Self::Value) -> Self {
    Self {
      base_supply,
      quote_supply,
      fee_bps,
      subsidy,
    }
  }

  fn store(self) -> Self::Value {
    (
      self.base_supply,
      self.quote_supply,
      self.fee_bps,
      self.subsidy,
    )
  }
}

pub type RelicEntryValue = (
  u64,                     // block
  (u128, u128),            // enshrining
  u16,                     // fee
  u64,                     // number
  SpacedRelicValue,        // spaced_relic
  Option<char>,            // symbol
  Option<u32>,             // owner sequence number
  Option<BoostTermsValue>, // boost_terms
  Option<MintTermsValue>,  // mint_terms
  RelicStateValue,         // state
  Option<PoolValue>,       // pool
  u64,                     // timestamp
);

impl Default for RelicEntry {
  fn default() -> Self {
    Self {
      block: 0,
      enshrining: Txid::all_zeros(),
      fee: 100, // 1%
      number: 0,
      spaced_relic: SpacedRelic::default(),
      symbol: None,
      owner_sequence_number: None,
      boost_terms: None,
      mint_terms: None,
      state: RelicState::default(),
      pool: None,
      timestamp: 0,
    }
  }
}

impl Entry for RelicEntry {
  type Value = RelicEntryValue;

  fn load(
    (
      block,
      enshrining,
      fee,
      number,
      spaced_relic,
      symbol,
      owner_sequence_number,
      boost_terms,
      mint_terms,
      state,
      pool,
      timestamp,
    ): RelicEntryValue,
  ) -> Self {
    Self {
      block,
      enshrining: {
        let low = enshrining.0.to_le_bytes();
        let high = enshrining.1.to_le_bytes();
        let bytes: Vec<u8> = [low, high].concat();
        Txid::from_slice(bytes.as_slice()).unwrap_or(Txid::all_zeros())
      },
      fee,
      number,
      spaced_relic: SpacedRelic::load(spaced_relic),
      symbol,
      owner_sequence_number,
      boost_terms: boost_terms.map(BoostTerms::load),
      mint_terms: mint_terms.map(MintTerms::load),
      state: RelicState::load(state),
      pool: pool.map(Pool::load),
      timestamp,
    }
  }

  fn store(self) -> Self::Value {
    (
      self.block,
      {
        let bytes_vec = self.enshrining.to_byte_array();
        let bytes: [u8; 32] = match bytes_vec.len() {
          32 => {
            let mut array = [0; 32];
            array.copy_from_slice(&bytes_vec);
            array
          }
          _ => panic!("Vector length is not 32"),
        };
        (
          u128::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
          ]),
          u128::from_le_bytes([
            bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21], bytes[22], bytes[23],
            bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29], bytes[30], bytes[31],
          ]),
        )
      },
      self.fee,
      self.number,
      self.spaced_relic.store(),
      self.symbol,
      self.owner_sequence_number,
      self.boost_terms.map(|boost| boost.store()),
      self.mint_terms.map(|terms| terms.store()),
      self.state.store(),
      self.pool.map(|pool| pool.store()),
      self.timestamp,
    )
  }
}

pub type RelicIdValue = (u64, u32);

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn relic_entry() {
    let txid_bytes = [
      0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
      0x0F, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D,
      0x1E, 0x1F,
    ];
    let txid = Txid::from_slice(&txid_bytes).expect("Slice must be of correct length");
    let entry = RelicEntry {
      block: 12,
      enshrining: txid,
      fee: 0,
      number: 6,
      spaced_relic: SpacedRelic {
        relic: Relic(7),
        spacers: 8,
      },
      symbol: Some('a'),
      owner_sequence_number: Some(123),
      boost_terms: None,
      mint_terms: Some(MintTerms {
        amount: Some(4),
        block_cap: None,
        cap: Some(1),
        max_unmints: None,
        price: Some(PriceModel::Fixed(8)),
        seed: Some(22),
        tx_cap: None,
      }),
      state: RelicState {
        burned: 33,
        mints: 44,
        unmints: 17,
      },
      pool: Some(Pool {
        base_supply: 321,
        quote_supply: 123,
        fee_bps: 13,
        subsidy: 10_000,
      }),
      timestamp: 10,
    };

    let value = (
      12,
      (
        0x0F0E0D0C0B0A09080706050403020100,
        0x1F1E1D1C1B1A19181716151413121110,
      ),
      0,
      6,
      (7, 8),
      Some('a'),
      Some(123),
      None,
      Some((
        Some(4),
        None,
        Some(1),
        None,
        Some(1),
        Some(8),
        None,
        None,
        Some(22),
        None,
      )),
      (33, 44, 17),
      Some((321, 123, 13, 10_000)),
      10,
    );

    assert_eq!(entry.store(), value);
    assert_eq!(RelicEntry::load(value), entry);
  }

  #[test]
  fn relic_id_entry() {
    assert_eq!(RelicId { block: 1, tx: 2 }.store(), (1, 2),);
    assert_eq!(RelicId { block: 1, tx: 2 }, RelicId::load((1, 2)),);
  }

  #[test]
  fn mintable_default() {
    assert_eq!(
      RelicEntry::default().mintable(0, 1, 0),
      Err(RelicError::Unmintable)
    );
  }
}
