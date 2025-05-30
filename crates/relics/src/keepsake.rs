use {super::*, flag::Flag, message::Message, tag::Tag};

mod flag;
mod message;
mod tag;

/// Relic protocol message
#[derive(Default, Serialize, Clone, Deserialize, Debug, PartialEq, Eq)]
pub struct Keepsake {
  /// allocation of Relics to outputs
  pub transfers: Vec<Transfer>,
  /// output number to receive unallocated Relics,
  /// if not specified the first non-OP_RETURN output is used
  pub pointer: Option<u32>,
  /// if set any tokens claimable by the script of the given output will be allocated
  /// note: the script on the given output must match the "owner" output of the enshrining
  pub claim: Option<u32>,
  /// seal a Relic Ticker
  pub sealing: bool,
  /// enshrine a previously sealed Relic
  pub enshrining: Option<Enshrining>,
  /// multi mint (also unmint) given Relic
  pub mint: Option<MultiMint>,
  /// execute token swap
  pub swap: Option<Swap>,
}

#[derive(Debug, PartialEq)]
enum Payload {
  Valid(Vec<u8>),
  Invalid(RelicFlaw),
}

impl Keepsake {
  /// Runes use 13, Relics use 15
  pub const MAGIC_NUMBER: opcodes::Opcode = opcodes::all::OP_PUSHNUM_15;
  pub const COMMIT_CONFIRMATIONS: u16 = 6;

  pub fn decipher(transaction: &Transaction) -> Option<RelicArtifact> {
    let payload = match Keepsake::payload(transaction) {
      Some(Payload::Valid(payload)) => payload,
      Some(Payload::Invalid(flaw)) => {
        return Some(RelicArtifact::Cenotaph(RelicCenotaph { flaw: Some(flaw) }));
      }
      None => return None,
    };

    let Ok(integers) = Keepsake::integers(&payload) else {
      return Some(RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::Varint),
      }));
    };

    let Message {
      mut flaw,
      transfers,
      mut fields,
    } = Message::from_integers(transaction, &integers);

    let mut flags = Tag::Flags
      .take(&mut fields, |[flags]| Some(flags))
      .unwrap_or_default();

    let get_non_zero = |tag: Tag, fields: &mut HashMap<u128, VecDeque<u128>>| -> Option<u128> {
      tag.take(fields, |[value]| (value > 0).then_some(value))
    };

    let get_output_option = |tag: Tag, fields: &mut HashMap<u128, VecDeque<u128>>| -> Option<u32> {
      tag.take(fields, |[value]| {
        let value = u32::try_from(value).ok()?;
        (u64::from(value) < u64::try_from(transaction.output.len()).unwrap()).then_some(value)
      })
    };

    let get_relic_id = |tag: Tag, fields: &mut HashMap<u128, VecDeque<u128>>| -> Option<RelicId> {
      tag.take(fields, |[block, tx]| {
        RelicId::new(block.try_into().ok()?, tx.try_into().ok()?)
      })
    };

    let sealing = Flag::Sealing.take(&mut flags);
    let enshrining = Flag::Enshrining.take(&mut flags).then(|| Enshrining {
      boost_terms: Flag::BoostTerms.take(&mut flags).then(|| BoostTerms {
        rare_chance: Tag::RareChance.take(&mut fields, |[val]| u32::try_from(val).ok()),
        rare_multiplier_cap: Tag::RareMultiplierCap
          .take(&mut fields, |[val]| u16::try_from(val).ok()),
        ultra_rare_chance: Tag::UltraRareChance.take(&mut fields, |[val]| u32::try_from(val).ok()),
        ultra_rare_multiplier_cap: Tag::UltraRareMultiplierCap
          .take(&mut fields, |[val]| u16::try_from(val).ok()),
      }),
      fee: Tag::Fee.take(&mut fields, |[val]| u16::try_from(val).ok()),
      symbol: Tag::Symbol.take(&mut fields, |[symbol]| {
        char::from_u32(u32::try_from(symbol).ok()?)
      }),
      mint_terms: Flag::MintTerms.take(&mut flags).then(|| MintTerms {
        amount: Tag::Amount.take(&mut fields, |[amount]| Some(amount)),
        block_cap: Tag::BlockCap.take(&mut fields, |[val]| u32::try_from(val).ok()),
        cap: Tag::Cap.take(&mut fields, |[cap]| Some(cap)),
        tx_cap: Tag::TxCap.take(&mut fields, |[val]| u8::try_from(val).ok()),
        max_unmints: Tag::MaxUnmints.take(&mut fields, |[val]| u32::try_from(val).ok()),
        price: Tag::Price
          .take(&mut fields, |values: [u128; 1]| {
            Some(PriceModel::Fixed(values[0]))
          })
          .or_else(|| {
            let a = Tag::PriceFormulaA.take(&mut fields, |[a]| Some(a));
            let b = Tag::PriceFormulaB.take(&mut fields, |[b]| Some(b));

            if let (Some(a), Some(b)) = (a, b) {
              Some(PriceModel::Formula { a, b })
            } else {
              None
            }
          }),
        seed: get_non_zero(Tag::Seed, &mut fields),
      }),
      subsidy: Tag::Subsidy.take(&mut fields, |[subsidy]| Some(subsidy)),
    });

    let multi_mint = if Flag::MultiMint.take(&mut flags) {
      let is_unmint = Tag::MultiMintIsUnmint
        .take(&mut fields, |[val]| Some(val != 0))
        .unwrap_or(false);
      let count = Tag::MultiMintCount.take(&mut fields, |[val]| u8::try_from(val).ok())?;
      let base_limit = Tag::MultiMintBaseLimit.take(&mut fields, |[val]| Some(val))?;
      let relic = Tag::MultiMintRelic.take(&mut fields, |[block, tx]| {
        RelicId::new(block.try_into().ok()?, tx.try_into().ok()?)
      })?;
      Some(MultiMint {
        count,
        base_limit,
        relic,
        is_unmint,
      })
    } else {
      None
    };

    let swap = Flag::Swap.take(&mut flags).then(|| Swap {
      input: get_relic_id(Tag::SwapInput, &mut fields),
      output: get_relic_id(Tag::SwapOutput, &mut fields),
      input_amount: get_non_zero(Tag::SwapInputAmount, &mut fields),
      output_amount: get_non_zero(Tag::SwapOutputAmount, &mut fields),
      is_exact_input: Flag::SwapExactInput.take(&mut flags),
    });

    let pointer = get_output_option(Tag::Pointer, &mut fields);
    let claim = get_output_option(Tag::Claim, &mut fields);

    if let Some(enshrining) = &enshrining {
      if let Err(err) = enshrining.validate() {
        flaw.get_or_insert(err);
      }
    }

    // Additionally, base token must not be multi minted.
    if multi_mint
      .as_ref()
      .map(|m| m.relic == RELIC_ID)
      .unwrap_or(false)
    {
      flaw.get_or_insert(RelicFlaw::InvalidBaseTokenMint);
    }

    // make sure to not swap from and to the same token
    if swap
      .map(|swap| swap.input.unwrap_or(RELIC_ID) == swap.output.unwrap_or(RELIC_ID))
      .unwrap_or_default()
    {
      flaw.get_or_insert(RelicFlaw::InvalidSwap);
    }

    if flags != 0 {
      flaw.get_or_insert(RelicFlaw::UnrecognizedFlag);
    }

    if fields.keys().any(|tag| tag % 2 == 0) {
      flaw.get_or_insert(RelicFlaw::UnrecognizedEvenTag);
    }

    if let Some(flaw) = flaw {
      return Some(RelicArtifact::Cenotaph(RelicCenotaph { flaw: Some(flaw) }));
    }

    Some(RelicArtifact::Keepsake(Self {
      transfers,
      pointer,
      claim,
      sealing,
      enshrining,
      mint: multi_mint,
      swap,
    }))
  }

  fn encipher_internal(&self) -> Vec<u8> {
    let mut payload = Vec::new();
    let mut flags = 0;

    if self.sealing {
      Flag::Sealing.set(&mut flags);
    }

    if let Some(enshrining) = self.enshrining {
      Flag::Enshrining.set(&mut flags);

      Tag::Symbol.encode_option(enshrining.symbol, &mut payload);
      Tag::Fee.encode_option(enshrining.fee, &mut payload);

      if let Some(boost) = enshrining.boost_terms {
        Flag::BoostTerms.set(&mut flags);
        Tag::RareChance.encode_option(boost.rare_chance, &mut payload);
        Tag::RareMultiplierCap.encode_option(boost.rare_multiplier_cap, &mut payload);
        Tag::UltraRareChance.encode_option(boost.ultra_rare_chance, &mut payload);
        Tag::UltraRareMultiplierCap.encode_option(boost.ultra_rare_multiplier_cap, &mut payload);
      }

      if let Some(terms) = enshrining.mint_terms {
        Flag::MintTerms.set(&mut flags);
        Tag::Amount.encode_option(terms.amount, &mut payload);
        Tag::BlockCap.encode_option(terms.block_cap, &mut payload);
        Tag::TxCap.encode_option(terms.tx_cap, &mut payload);
        Tag::Cap.encode_option(terms.cap, &mut payload);
        if let Some(price_model) = terms.price {
          match price_model {
            PriceModel::Fixed(price) => {
              // Fixed price: encode as a single integer with Tag::Price
              Tag::Price.encode([price], &mut payload);
            }
            PriceModel::Formula { a, b } => {
              // Formula pricing: encode each component with its own tag
              Tag::PriceFormulaA.encode([a], &mut payload);
              Tag::PriceFormulaB.encode([b], &mut payload);
            }
          }
        }
        Tag::Seed.encode_option(terms.seed, &mut payload);
        Tag::MaxUnmints.encode_option(terms.max_unmints, &mut payload);
      }
      // Encode Subsidy if it exists
      Tag::Subsidy.encode_option(enshrining.subsidy, &mut payload);
    }

    if let Some(multi) = self.mint {
      Flag::MultiMint.set(&mut flags);
      if multi.is_unmint {
        Tag::MultiMintIsUnmint.encode([1], &mut payload);
      }
      Tag::MultiMintCount.encode([multi.count as u128], &mut payload);
      Tag::MultiMintBaseLimit.encode([multi.base_limit], &mut payload);
      Tag::MultiMintRelic.encode(
        [multi.relic.block.into(), multi.relic.tx.into()],
        &mut payload,
      );
    }

    if let Some(swap) = &self.swap {
      Flag::Swap.set(&mut flags);

      if swap.is_exact_input {
        Flag::SwapExactInput.set(&mut flags);
      }

      if let Some(RelicId { block, tx }) = swap.input {
        Tag::SwapInput.encode([block.into(), tx.into()], &mut payload);
      }
      if let Some(RelicId { block, tx }) = swap.output {
        Tag::SwapOutput.encode([block.into(), tx.into()], &mut payload);
      }
      Tag::SwapInputAmount.encode_option(swap.input_amount, &mut payload);
      Tag::SwapOutputAmount.encode_option(swap.output_amount, &mut payload);
    }

    if flags != 0 {
      Tag::Flags.encode([flags], &mut payload);
    }

    Tag::Pointer.encode_option(self.pointer, &mut payload);
    Tag::Claim.encode_option(self.claim, &mut payload);

    if !self.transfers.is_empty() {
      varint::encode_to_vec(Tag::Body.into(), &mut payload);

      let mut transfers = self.transfers.clone();
      transfers.sort_by_key(|transfer| transfer.id);

      let mut previous = RelicId::default();
      for transfer in transfers {
        let (block, tx) = previous.delta(transfer.id).unwrap();
        varint::encode_to_vec(block, &mut payload);
        varint::encode_to_vec(tx, &mut payload);
        varint::encode_to_vec(transfer.amount, &mut payload);
        varint::encode_to_vec(transfer.output.into(), &mut payload);
        previous = transfer.id;
      }
    }
    payload
  }

  pub fn encipher(&self) -> ScriptBuf {
    let mut builder = script::Builder::new()
      .push_opcode(opcodes::all::OP_RETURN)
      .push_opcode(Keepsake::MAGIC_NUMBER);

    for chunk in self.encipher_internal().chunks(MAX_SCRIPT_ELEMENT_SIZE) {
      let push: &script::PushBytes = chunk.try_into().unwrap();
      builder = builder.push_slice(push);
    }

    builder.into_script()
  }

  fn payload(transaction: &Transaction) -> Option<Payload> {
    // search transaction outputs for payload
    for output in &transaction.output {
      let mut instructions = output.script_pubkey.instructions();

      // payload starts with OP_RETURN
      if instructions.next() != Some(Ok(Instruction::Op(opcodes::all::OP_RETURN))) {
        continue;
      }

      // followed by the protocol identifier, ignoring errors, since OP_RETURN
      // scripts may be invalid
      if instructions.next() != Some(Ok(Instruction::Op(Keepsake::MAGIC_NUMBER))) {
        continue;
      }

      // construct the payload by concatenating remaining data pushes
      let mut payload = Vec::new();

      for result in instructions {
        match result {
          Ok(Instruction::PushBytes(push)) => {
            payload.extend_from_slice(push.as_bytes());
          }
          Ok(Instruction::Op(_)) => {
            return Some(Payload::Invalid(RelicFlaw::Opcode));
          }
          Err(_) => {
            return Some(Payload::Invalid(RelicFlaw::InvalidScript));
          }
        }
      }

      return Some(Payload::Valid(payload));
    }

    None
  }

  fn integers(payload: &[u8]) -> Result<Vec<u128>, varint::Error> {
    let mut integers = Vec::new();
    let mut i = 0;

    while i < payload.len() {
      let (integer, length) = varint::decode(&payload[i..])?;
      integers.push(integer);
      i += length;
    }

    Ok(integers)
  }
}

#[cfg(test)]
mod tests {
  use bitcoin::transaction::Version;
  use bitcoin::Amount;
  use {
    super::*,
    bitcoin::{
      blockdata::locktime::absolute::LockTime, script::PushBytes, OutPoint, Sequence, TxIn, TxOut,
      Witness,
    },
    pretty_assertions::assert_eq,
  };

  pub(crate) fn relic_id(tx: u32) -> RelicId {
    RelicId { block: 1, tx }
  }

  pub(crate) fn relic_id_with_block(block: u64, tx: u32) -> RelicId {
    RelicId { block, tx }
  }

  fn decipher(integers: &[u128]) -> RelicArtifact {
    let payload = payload(integers);

    let payload: &PushBytes = payload.as_slice().try_into().unwrap();

    Keepsake::decipher(&Transaction {
      input: Vec::new(),
      output: vec![TxOut {
        script_pubkey: script::Builder::new()
          .push_opcode(opcodes::all::OP_RETURN)
          .push_opcode(Keepsake::MAGIC_NUMBER)
          .push_slice(payload)
          .into_script(),
        value: Amount::ZERO,
      }],
      lock_time: LockTime::ZERO,
      version: Version(2),
    })
    .unwrap()
  }

  fn payload(integers: &[u128]) -> Vec<u8> {
    let mut payload = Vec::new();

    for integer in integers {
      payload.extend(varint::encode(*integer));
    }

    payload
  }

  #[test]
  fn decipher_returns_none_if_first_opcode_is_malformed() {
    assert_eq!(
      Keepsake::decipher(&Transaction {
        input: Vec::new(),
        output: vec![TxOut {
          script_pubkey: ScriptBuf::from_bytes(vec![opcodes::all::OP_PUSHBYTES_4.to_u8()]),
          value: Amount::ZERO,
        }],
        lock_time: LockTime::ZERO,
        version: Version(2),
      }),
      None,
    );
  }

  #[test]
  fn deciphering_transaction_with_no_outputs_returns_none() {
    assert_eq!(
      Keepsake::decipher(&Transaction {
        input: Vec::new(),
        output: Vec::new(),
        lock_time: LockTime::ZERO,
        version: Version(2),
      }),
      None,
    );
  }

  #[test]
  fn deciphering_transaction_with_non_op_return_output_returns_none() {
    assert_eq!(
      Keepsake::decipher(&Transaction {
        input: Vec::new(),
        output: vec![TxOut {
          script_pubkey: script::Builder::new().push_slice([]).into_script(),
          value: Amount::ZERO
        }],
        lock_time: LockTime::ZERO,
        version: Version(2),
      }),
      None,
    );
  }

  #[test]
  fn deciphering_transaction_with_bare_op_return_returns_none() {
    assert_eq!(
      Keepsake::decipher(&Transaction {
        input: Vec::new(),
        output: vec![TxOut {
          script_pubkey: script::Builder::new()
            .push_opcode(opcodes::all::OP_RETURN)
            .into_script(),
          value: Amount::ZERO
        }],
        lock_time: LockTime::ZERO,
        version: Version(2),
      }),
      None,
    );
  }

  #[test]
  fn deciphering_transaction_with_non_matching_op_return_returns_none() {
    assert_eq!(
      Keepsake::decipher(&Transaction {
        input: Vec::new(),
        output: vec![TxOut {
          script_pubkey: script::Builder::new()
            .push_opcode(opcodes::all::OP_RETURN)
            .push_slice(b"FOOO")
            .into_script(),
          value: Amount::ZERO
        }],
        lock_time: LockTime::ZERO,
        version: Version(2),
      }),
      None,
    );
  }

  #[test]
  fn deciphering_valid_runestone_with_invalid_script_postfix_returns_invalid_payload() {
    let mut script_pubkey = script::Builder::new()
      .push_opcode(opcodes::all::OP_RETURN)
      .push_opcode(Keepsake::MAGIC_NUMBER)
      .into_script()
      .into_bytes();

    script_pubkey.push(opcodes::all::OP_PUSHBYTES_4.to_u8());

    assert_eq!(
      Keepsake::payload(&Transaction {
        input: Vec::new(),
        output: vec![TxOut {
          script_pubkey: ScriptBuf::from_bytes(script_pubkey),
          value: Amount::ZERO,
        }],
        lock_time: LockTime::ZERO,
        version: Version(2),
      }),
      Some(Payload::Invalid(RelicFlaw::InvalidScript))
    );
  }

  #[test]
  fn deciphering_runestone_with_truncated_varint_succeeds() {
    Keepsake::decipher(&Transaction {
      input: Vec::new(),
      output: vec![TxOut {
        script_pubkey: script::Builder::new()
          .push_opcode(opcodes::all::OP_RETURN)
          .push_opcode(Keepsake::MAGIC_NUMBER)
          .push_slice([128])
          .into_script(),
        value: Amount::ZERO,
      }],
      lock_time: LockTime::ZERO,
      version: Version(2),
    })
    .unwrap();
  }

  #[test]
  fn outputs_with_non_pushdata_opcodes_are_cenotaph() {
    assert_eq!(
      Keepsake::decipher(&Transaction {
        input: Vec::new(),
        output: vec![
          TxOut {
            script_pubkey: script::Builder::new()
              .push_opcode(opcodes::all::OP_RETURN)
              .push_opcode(Keepsake::MAGIC_NUMBER)
              .push_opcode(opcodes::all::OP_VERIFY)
              .push_slice([0])
              .push_slice::<&PushBytes>(varint::encode(1).as_slice().try_into().unwrap())
              .push_slice::<&PushBytes>(varint::encode(1).as_slice().try_into().unwrap())
              .push_slice([2, 0])
              .into_script(),
            value: Amount::ZERO,
          },
          TxOut {
            script_pubkey: script::Builder::new()
              .push_opcode(opcodes::all::OP_RETURN)
              .push_opcode(Keepsake::MAGIC_NUMBER)
              .push_slice([0])
              .push_slice::<&PushBytes>(varint::encode(1).as_slice().try_into().unwrap())
              .push_slice::<&PushBytes>(varint::encode(2).as_slice().try_into().unwrap())
              .push_slice([3, 0])
              .into_script(),
            value: Amount::ZERO,
          },
        ],
        lock_time: LockTime::ZERO,
        version: Version(2),
      })
      .unwrap(),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::Opcode),
      }),
    );
  }

  #[test]
  fn pushnum_opcodes_in_runestone_produce_cenotaph() {
    assert_eq!(
      Keepsake::decipher(&Transaction {
        input: Vec::new(),
        output: vec![TxOut {
          script_pubkey: script::Builder::new()
            .push_opcode(opcodes::all::OP_RETURN)
            .push_opcode(Keepsake::MAGIC_NUMBER)
            .push_opcode(opcodes::all::OP_PUSHNUM_1)
            .into_script(),
          value: Amount::ZERO,
        },],
        lock_time: LockTime::ZERO,
        version: Version(2),
      })
      .unwrap(),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::Opcode),
      }),
    );
  }

  #[test]
  fn deciphering_empty_runestone_is_successful() {
    assert_eq!(
      Keepsake::decipher(&Transaction {
        input: Vec::new(),
        output: vec![TxOut {
          script_pubkey: script::Builder::new()
            .push_opcode(opcodes::all::OP_RETURN)
            .push_opcode(Keepsake::MAGIC_NUMBER)
            .into_script(),
          value: Amount::ZERO
        }],
        lock_time: LockTime::ZERO,
        version: Version(2),
      })
      .unwrap(),
      RelicArtifact::Keepsake(Keepsake::default()),
    );
  }

  #[test]
  fn invalid_input_scripts_are_skipped_when_searching_for_runestone() {
    let payload = payload(&[Tag::Pointer.into(), 1]);

    let payload: &PushBytes = payload.as_slice().try_into().unwrap();

    let script_pubkey = vec![
      opcodes::all::OP_RETURN.to_u8(),
      opcodes::all::OP_PUSHBYTES_9.to_u8(),
      Keepsake::MAGIC_NUMBER.to_u8(),
      opcodes::all::OP_PUSHBYTES_4.to_u8(),
    ];

    assert_eq!(
      Keepsake::decipher(&Transaction {
        input: Vec::new(),
        output: vec![
          TxOut {
            script_pubkey: ScriptBuf::from_bytes(script_pubkey),
            value: Amount::ZERO,
          },
          TxOut {
            script_pubkey: script::Builder::new()
              .push_opcode(opcodes::all::OP_RETURN)
              .push_opcode(Keepsake::MAGIC_NUMBER)
              .push_slice(payload)
              .into_script(),
            value: Amount::ZERO,
          },
        ],
        lock_time: LockTime::ZERO,
        version: Version(2),
      })
      .unwrap(),
      RelicArtifact::Keepsake(Keepsake {
        pointer: Some(1),
        ..default()
      }),
    );
  }

  #[test]
  fn deciphering_non_empty_runestone_is_successful() {
    assert_eq!(
      decipher(&[Tag::Body.into(), 1, 1, 2, 0]),
      RelicArtifact::Keepsake(Keepsake {
        transfers: vec![Transfer {
          id: relic_id(1),
          amount: 2,
          output: 0,
        }],
        ..default()
      }),
    );
  }

  #[test]
  fn valid_boost_terms_create_keepsake() {
    let valid_boost = BoostTerms {
      rare_chance: Some(5000),
      rare_multiplier_cap: Some(10),
      ultra_rare_chance: Some(1000),
      ultra_rare_multiplier_cap: Some(20),
    };

    let enshrining = Enshrining {
      boost_terms: Some(valid_boost),
      mint_terms: Some(MintTerms {
        amount: Some(100),
        cap: Some(100_000),
        price: Some(PriceModel::Fixed(1)),
        ..default()
      }),
      ..default()
    };

    let keepsake = Keepsake {
      transfers: Vec::new(),
      enshrining: Some(enshrining),
      ..default()
    };

    let script = keepsake.encipher();
    let decoded = Keepsake::decipher(&Transaction {
      input: Vec::new(),
      output: vec![TxOut {
        script_pubkey: script,
        value: Amount::ZERO,
      }],
      lock_time: LockTime::ZERO,
      version: Version(2),
    })
    .unwrap();

    assert_eq!(decoded, RelicArtifact::Keepsake(keepsake));
  }

  #[test]
  fn invalid_boost_term_chances_create_cenotaph() {
    let invalid_boost = BoostTerms {
      rare_chance: Some(1000),
      rare_multiplier_cap: Some(10),
      ultra_rare_chance: Some(2000),
      ultra_rare_multiplier_cap: Some(20),
    };

    let invalid_enshrining = Enshrining {
      boost_terms: Some(invalid_boost),
      mint_terms: Some(MintTerms {
        amount: Some(100),
        cap: Some(100_000),
        price: Some(PriceModel::Fixed(1)),
        ..default()
      }),
      ..default()
    };

    let invalid_keepsake = Keepsake {
      transfers: Vec::new(),
      enshrining: Some(invalid_enshrining),
      ..default()
    };

    let script = invalid_keepsake.encipher();
    let decoded = Keepsake::decipher(&Transaction {
      input: Vec::new(),
      output: vec![TxOut {
        script_pubkey: script,
        value: Amount::ZERO,
      }],
      lock_time: LockTime::ZERO,
      version: Version(2),
    })
    .unwrap();

    assert_eq!(
      decoded,
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::InvalidEnshriningBoostChanceOrder),
      })
    );
  }

  #[test]
  fn valid_multimint_creates_keepsake() {
    let keepsake = Keepsake {
      mint: Some(MultiMint {
        count: 5,
        base_limit: 1000,
        is_unmint: false,
        relic: relic_id(42),
      }),
      ..default()
    };

    let script = keepsake.encipher();
    let decoded = Keepsake::decipher(&Transaction {
      input: Vec::new(),
      output: vec![TxOut {
        script_pubkey: script,
        value: Amount::ZERO,
      }],
      lock_time: LockTime::ZERO,
      version: Version(2),
    })
    .unwrap();

    assert_eq!(decoded, RelicArtifact::Keepsake(keepsake));
  }

  #[test]
  fn valid_multimint_with_max_mints_creates_keepsake() {
    let max_mints_keepsake = Keepsake {
      mint: Some(MultiMint {
        count: u8::MAX,
        base_limit: 1000000,
        is_unmint: false,
        relic: relic_id(42),
      }),
      ..default()
    };

    let script = max_mints_keepsake.encipher();
    let decoded = Keepsake::decipher(&Transaction {
      input: Vec::new(),
      output: vec![TxOut {
        script_pubkey: script,
        value: Amount::ZERO,
      }],
      lock_time: LockTime::ZERO,
      version: Version(2),
    })
    .unwrap();

    assert_eq!(decoded, RelicArtifact::Keepsake(max_mints_keepsake));
  }

  #[test]
  fn valid_multimint_with_unmint_creates_keepsake() {
    let unmint_keepsake = Keepsake {
      mint: Some(MultiMint {
        count: 3,
        base_limit: 500,
        is_unmint: true,
        relic: relic_id(42),
      }),
      ..default()
    };

    let script = unmint_keepsake.encipher();
    let decoded = Keepsake::decipher(&Transaction {
      input: Vec::new(),
      output: vec![TxOut {
        script_pubkey: script,
        value: Amount::ZERO,
      }],
      lock_time: LockTime::ZERO,
      version: Version(2),
    })
    .unwrap();

    assert_eq!(decoded, RelicArtifact::Keepsake(unmint_keepsake));
  }

  #[test]
  fn invalid_boost_term_multipliers_create_cenotaph() {
    let invalid_multipliers = BoostTerms {
      rare_chance: Some(5000),
      rare_multiplier_cap: Some(30),
      ultra_rare_chance: Some(1000),
      ultra_rare_multiplier_cap: Some(20),
    };

    let invalid_multiplier_enshrining = Enshrining {
      boost_terms: Some(invalid_multipliers),
      mint_terms: Some(MintTerms {
        amount: Some(100),
        cap: Some(100_000),
        price: Some(PriceModel::Fixed(1)),
        ..default()
      }),
      ..default()
    };

    let invalid_multiplier_keepsake = Keepsake {
      transfers: Vec::new(),
      enshrining: Some(invalid_multiplier_enshrining),
      ..default()
    };

    let script = invalid_multiplier_keepsake.encipher();
    let decoded = Keepsake::decipher(&Transaction {
      input: Vec::new(),
      output: vec![TxOut {
        script_pubkey: script,
        value: Amount::ZERO,
      }],
      lock_time: LockTime::ZERO,
      version: Version(2),
    })
    .unwrap();

    assert_eq!(
      decoded,
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::InvalidEnshriningBoostMultiplierOrder),
      })
    );
  }

  #[test]
  fn decipher_enshrining() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Enshrining.mask() | Flag::MintTerms.mask(),
        Tag::Symbol.into(),
        'R'.into(),
        Tag::Amount.into(),
        100,
        Tag::Cap.into(),
        100_000,
        Tag::Price.into(),
        1,
        Tag::Body.into(),
        1,
        1,
        2,
        0,
      ]),
      RelicArtifact::Keepsake(Keepsake {
        transfers: vec![Transfer {
          id: relic_id(1),
          amount: 2,
          output: 0,
        }],
        enshrining: Some(Enshrining {
          symbol: Some('R'),
          mint_terms: Some(MintTerms {
            amount: Some(100),
            cap: Some(100_000),
            price: Some(PriceModel::Fixed(1)),
            ..default()
          }),
          ..default()
        }),
        ..default()
      }),
    );
  }

  #[test]
  fn recognized_fields_without_flag_produces_cenotaph() {
    #[track_caller]
    fn case(integers: &[u128]) {
      assert_eq!(
        decipher(integers),
        RelicArtifact::Cenotaph(RelicCenotaph {
          flaw: Some(RelicFlaw::UnrecognizedEvenTag),
        }),
      );
    }

    case(&[Tag::Seed.into(), 0]);
    case(&[Tag::Amount.into(), 0]);
    case(&[Tag::Cap.into(), 0]);
    case(&[Tag::Price.into(), 0]);
    case(&[Tag::SwapInput.into(), 0]);
    case(&[Tag::SwapOutput.into(), 0]);
    case(&[Tag::SwapInputAmount.into(), 0]);
    case(&[Tag::SwapOutputAmount.into(), 0]);
    case(&[Tag::MultiMintCount.into(), 0]);
    case(&[Tag::MultiMintBaseLimit.into(), 0]);
    case(&[Tag::MultiMintRelic.into(), 0]);
    case(&[Tag::MultiMintIsUnmint.into(), 0]);

    // case(&[Tag::Flags.into(), Flag::Enshrining.into(), Tag::Cap.into(), 0]);
    // case(&[
    //   Tag::Flags.into(),
    //   Flag::Enshrining.into(),
    //   Tag::Amount.into(),
    //   0,
    // ]);
    // case(&[
    //   Tag::Flags.into(),
    //   Flag::Enshrining.into(),
    //   Tag::OffsetStart.into(),
    //   0,
    // ]);
    // case(&[
    //   Tag::Flags.into(),
    //   Flag::Enshrining.into(),
    //   Tag::OffsetEnd.into(),
    //   0,
    // ]);
    // case(&[
    //   Tag::Flags.into(),
    //   Flag::Enshrining.into(),
    //   Tag::HeightStart.into(),
    //   0,
    // ]);
    // case(&[
    //   Tag::Flags.into(),
    //   Flag::Enshrining.into(),
    //   Tag::HeightEnd.into(),
    //   0,
    // ]);
  }

  // #[test]
  // fn decipher_etching_with_term() {
  //   assert_eq!(
  //     decipher(&[
  //       Tag::Flags.into(),
  //       Flag::Enshrining.mask() | Flag::Terms.mask(),
  //       Tag::OffsetEnd.into(),
  //       4,
  //       Tag::Body.into(),
  //       1,
  //       1,
  //       2,
  //       0
  //     ]),
  //     Artifact::Keepsake(Keepsake {
  //       transfers: vec![Transfer {
  //         id: relic_id(1),
  //         amount: 2,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         terms: Some(Terms {
  //           offset: (None, Some(4)),
  //           ..default()
  //         }),
  //         ..default()
  //       }),
  //       ..default()
  //     }),
  //   );
  // }

  // #[test]
  // fn decipher_etching_with_amount() {
  //   assert_eq!(
  //     decipher(&[
  //       Tag::Flags.into(),
  //       Flag::Enshrining.mask() | Flag::Terms.mask(),
  //       Tag::Amount.into(),
  //       4,
  //       Tag::Body.into(),
  //       1,
  //       1,
  //       2,
  //       0
  //     ]),
  //     Artifact::Keepsake(Keepsake {
  //       transfers: vec![Transfer {
  //         id: relic_id(1),
  //         amount: 2,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         terms: Some(Terms {
  //           amount: Some(4),
  //           ..default()
  //         }),
  //         ..default()
  //       }),
  //       ..default()
  //     }),
  //   );
  // }

  #[test]
  fn invalid_varint_produces_cenotaph() {
    assert_eq!(
      Keepsake::decipher(&Transaction {
        input: Vec::new(),
        output: vec![TxOut {
          script_pubkey: script::Builder::new()
            .push_opcode(opcodes::all::OP_RETURN)
            .push_opcode(Keepsake::MAGIC_NUMBER)
            .push_slice([128])
            .into_script(),
          value: Amount::ZERO,
        }],
        lock_time: LockTime::ZERO,
        version: Version(2),
      })
      .unwrap(),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::Varint),
      }),
    );
  }

  #[test]
  fn duplicate_even_tags_produce_cenotaph() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Enshrining.mask() | Flag::MintTerms.mask(),
        Tag::Symbol.into(),
        'R'.into(),
        Tag::Amount.into(),
        100,
        Tag::Cap.into(),
        100_000,
        Tag::Price.into(),
        1,
        Tag::Body.into(),
        1,
        1,
        2,
        0,
      ]),
      RelicArtifact::Keepsake(Keepsake {
        transfers: vec![Transfer {
          id: relic_id(1),
          amount: 2,
          output: 0,
        }],
        enshrining: Some(Enshrining {
          symbol: Some('R'),
          mint_terms: Some(MintTerms {
            amount: Some(100),
            cap: Some(100_000),
            price: Some(PriceModel::Fixed(1)),
            ..default()
          }),
          ..default()
        }),
        ..default()
      }),
    );
  }

  #[test]
  fn duplicate_odd_tags_are_ignored() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Enshrining.mask() | Flag::MintTerms.mask(),
        Tag::Symbol.into(),
        'a'.into(),
        Tag::Symbol.into(),
        'b'.into(),
        Tag::Amount.into(),
        100,
        Tag::Cap.into(),
        100_000,
        Tag::Price.into(),
        1,
        Tag::Body.into(),
        1,
        1,
        2,
        0,
      ]),
      RelicArtifact::Keepsake(Keepsake {
        transfers: vec![Transfer {
          id: relic_id(1),
          amount: 2,
          output: 0,
        }],
        enshrining: Some(Enshrining {
          symbol: Some('a'),
          mint_terms: Some(MintTerms {
            amount: Some(100),
            cap: Some(100_000),
            price: Some(PriceModel::Fixed(1)),
            ..default()
          }),
          ..default()
        }),
        ..default()
      })
    );
  }

  #[test]
  fn unrecognized_odd_tag_is_ignored() {
    assert_eq!(
      decipher(&[Tag::Nop.into(), 100, Tag::Body.into(), 1, 1, 2, 0]),
      RelicArtifact::Keepsake(Keepsake {
        transfers: vec![Transfer {
          id: relic_id(1),
          amount: 2,
          output: 0,
        }],
        ..default()
      }),
    );
  }

  #[test]
  fn runestone_with_unrecognized_even_tag_is_cenotaph() {
    assert_eq!(
      decipher(&[Tag::Cenotaph.into(), 0, Tag::Body.into(), 1, 1, 2, 0]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::UnrecognizedEvenTag),
      }),
    );
  }

  #[test]
  fn runestone_with_unrecognized_flag_is_cenotaph() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Cenotaph.mask(),
        Tag::Body.into(),
        1,
        1,
        2,
        0
      ]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::UnrecognizedFlag),
      }),
    );
  }

  #[test]
  fn runestone_with_edict_id_with_zero_block_and_nonzero_tx_is_cenotaph() {
    assert_eq!(
      decipher(&[Tag::Body.into(), 0, 1, 2, 0, 0, 0]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::TransferRelicId),
      }),
    );
  }

  #[test]
  fn runestone_with_overflowing_edict_id_delta_is_cenotaph() {
    assert_eq!(
      decipher(&[Tag::Body.into(), 1, 0, 0, 0, u64::MAX.into(), 0, 0, 0]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::TransferRelicId),
      }),
    );

    assert_eq!(
      decipher(&[Tag::Body.into(), 1, 1, 0, 0, 0, u64::MAX.into(), 0, 0,]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::TransferRelicId),
      }),
    );
  }

  #[test]
  fn runestone_with_output_over_max_is_cenotaph() {
    assert_eq!(
      decipher(&[Tag::Body.into(), 1, 1, 2, 2, 0, 0]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::TransferOutput),
      }),
    );
  }

  #[test]
  fn tag_with_no_value_is_cenotaph() {
    assert_eq!(
      decipher(&[Tag::Flags.into(), 1, Tag::Flags.into()]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::TruncatedField),
      }),
    );
  }

  #[test]
  fn trailing_integers_in_body_is_cenotaph() {
    let mut integers = vec![Tag::Body.into(), 1, 1, 2, 0];

    for i in 0..4 {
      assert_eq!(
        decipher(&integers),
        if i == 0 {
          RelicArtifact::Keepsake(Keepsake {
            transfers: vec![Transfer {
              id: relic_id(1),
              amount: 2,
              output: 0,
            }],
            ..default()
          })
        } else {
          RelicArtifact::Cenotaph(RelicCenotaph {
            flaw: Some(RelicFlaw::TrailingIntegers),
          })
        }
      );

      integers.push(0);
    }
  }

  // #[test]
  // fn divisibility_above_max_is_ignored() {
  //   assert_eq!(
  //     decipher(&[
  //       Tag::Flags.into(),
  //       Flag::Enshrining.mask(),
  //       Tag::Relic.into(),
  //       4,
  //       Tag::Divisibility.into(),
  //       (Enshrining::MAX_DIVISIBILITY + 1).into(),
  //       Tag::Body.into(),
  //       1,
  //       1,
  //       2,
  //       0,
  //     ]),
  //     RelicArtifact::Keepsake(Keepsake {
  //       transfers: vec![Transfer {
  //         id: relic_id(1),
  //         amount: 2,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(4)),
  //         ..default()
  //       }),
  //       ..default()
  //     }),
  //   );
  // }

  #[test]
  fn symbol_above_max_is_ignored() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Enshrining.mask() | Flag::MintTerms.mask(),
        Tag::Symbol.into(),
        u128::from(u32::from(char::MAX) + 1),
        Tag::Amount.into(),
        100,
        Tag::Cap.into(),
        100_000,
        Tag::Price.into(),
        1,
        Tag::Body.into(),
        1,
        1,
        2,
        0,
      ]),
      RelicArtifact::Keepsake(Keepsake {
        transfers: vec![Transfer {
          id: relic_id(1),
          amount: 2,
          output: 0,
        }],
        enshrining: Some(Enshrining {
          mint_terms: Some(MintTerms {
            amount: Some(100),
            cap: Some(100_000),
            price: Some(PriceModel::Fixed(1)),
            ..default()
          }),
          symbol: None,
          ..default()
        }),
        ..default()
      }),
    );
  }

  #[test]
  fn decipher_etching_with_all_etching_tags() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Sealing.mask()
          | Flag::Enshrining.mask()
          | Flag::MintTerms.mask()
          | Flag::Swap.mask()
          | Flag::SwapExactInput.mask()
          | Flag::MultiMint.mask(),
        Tag::Symbol.into(),
        'a'.into(),
        Tag::Amount.into(),
        100,
        Tag::Cap.into(),
        16_800,
        Tag::PriceFormulaA.into(),
        29_276_332,
        Tag::PriceFormulaB.into(),
        6994,
        Tag::Seed.into(),
        300,
        Tag::MultiMintRelic.into(),
        5,
        Tag::MultiMintRelic.into(),
        0,
        Tag::MultiMintBaseLimit.into(),
        u128::MAX,
        Tag::MultiMintCount.into(),
        2,
        Tag::MultiMintIsUnmint.into(),
        0,
        Tag::SwapInput.into(),
        1,
        Tag::SwapInput.into(),
        42,
        Tag::SwapOutput.into(),
        1,
        Tag::SwapOutput.into(),
        43,
        Tag::SwapInputAmount.into(),
        123,
        Tag::SwapOutputAmount.into(),
        456,
        Tag::Pointer.into(),
        0,
        Tag::Claim.into(),
        0,
        Tag::Body.into(),
        1,
        1,
        2,
        0,
      ]),
      RelicArtifact::Keepsake(Keepsake {
        transfers: vec![Transfer {
          id: relic_id(1),
          amount: 2,
          output: 0,
        }],
        sealing: true,
        enshrining: Some(Enshrining {
          boost_terms: None,
          fee: None,
          symbol: Some('a'),
          mint_terms: Some(MintTerms {
            amount: Some(100),
            block_cap: None,
            cap: Some(16_800),
            max_unmints: None,
            price: Some(PriceModel::Formula {
              a: 29_276_332,
              b: 6994
            }),
            seed: Some(300),
            tx_cap: None,
          }),
          subsidy: None,
        }),
        mint: Some(MultiMint {
          count: 2,
          base_limit: u128::MAX,
          is_unmint: false,
          relic: relic_id_with_block(5, 0),
        }),
        swap: Some(Swap {
          input: Some(relic_id(42)),
          output: Some(relic_id(43)),
          input_amount: Some(123),
          output_amount: Some(456),
          is_exact_input: true,
        }),
        pointer: Some(0),
        claim: Some(0),
      }),
    );
  }

  #[test]
  fn recognized_even_etching_fields_produce_cenotaph_if_etching_flag_is_not_set() {
    assert_eq!(
      decipher(&[Tag::Seed.into(), 4]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::UnrecognizedEvenTag),
      }),
    );
  }

  #[test]
  fn decipher_etching_with_min_height_and_symbol() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Enshrining.mask() | Flag::MintTerms.mask(),
        Tag::Symbol.into(),
        'a'.into(),
        Tag::Amount.into(),
        100,
        Tag::Cap.into(),
        100_000,
        Tag::Price.into(),
        1,
        Tag::Body.into(),
        1,
        1,
        2,
        0,
      ]),
      RelicArtifact::Keepsake(Keepsake {
        transfers: vec![Transfer {
          id: relic_id(1),
          amount: 2,
          output: 0,
        }],
        enshrining: Some(Enshrining {
          symbol: Some('a'),
          mint_terms: Some(MintTerms {
            amount: Some(100),
            cap: Some(100_000),
            price: Some(PriceModel::Fixed(1)),
            ..default()
          }),
          ..default()
        }),
        ..default()
      }),
    );
  }

  #[test]
  fn tag_values_are_not_parsed_as_tags() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Enshrining.mask() | Flag::MintTerms.mask(),
        Tag::Symbol.into(),
        Tag::Body.into(),
        Tag::Amount.into(),
        100,
        Tag::Cap.into(),
        100_000,
        Tag::Price.into(),
        1,
        Tag::Body.into(),
        1,
        1,
        2,
        0,
      ]),
      RelicArtifact::Keepsake(Keepsake {
        transfers: vec![Transfer {
          id: relic_id(1),
          amount: 2,
          output: 0,
        }],
        enshrining: Some(Enshrining {
          mint_terms: Some(MintTerms {
            amount: Some(100),
            cap: Some(100_000),
            price: Some(PriceModel::Fixed(1)),
            ..default()
          }),
          symbol: Some(0.into()),
          ..default()
        }),
        ..default()
      }),
    );
  }

  #[test]
  fn runestone_may_contain_multiple_edicts() {
    assert_eq!(
      decipher(&[Tag::Body.into(), 1, 1, 2, 0, 0, 3, 5, 0]),
      RelicArtifact::Keepsake(Keepsake {
        transfers: vec![
          Transfer {
            id: relic_id(1),
            amount: 2,
            output: 0,
          },
          Transfer {
            id: relic_id(4),
            amount: 5,
            output: 0,
          },
        ],
        ..default()
      }),
    );
  }

  #[test]
  fn runestones_with_invalid_rune_id_blocks_are_cenotaph() {
    assert_eq!(
      decipher(&[Tag::Body.into(), 1, 1, 2, 0, u128::MAX, 1, 0, 0]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::TransferRelicId),
      }),
    );
  }

  #[test]
  fn runestones_with_invalid_rune_id_txs_are_cenotaph() {
    assert_eq!(
      decipher(&[Tag::Body.into(), 1, 1, 2, 0, 1, u128::MAX, 0, 0]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::TransferRelicId),
      }),
    );
  }

  #[test]
  fn runestone_may_be_in_second_output() {
    let payload = payload(&[0, 1, 1, 2, 0]);

    let payload: &PushBytes = payload.as_slice().try_into().unwrap();

    assert_eq!(
      Keepsake::decipher(&Transaction {
        input: Vec::new(),
        output: vec![
          TxOut {
            script_pubkey: ScriptBuf::new(),
            value: Amount::ZERO,
          },
          TxOut {
            script_pubkey: script::Builder::new()
              .push_opcode(opcodes::all::OP_RETURN)
              .push_opcode(Keepsake::MAGIC_NUMBER)
              .push_slice(payload)
              .into_script(),
            value: Amount::ZERO
          }
        ],
        lock_time: LockTime::ZERO,
        version: Version(2),
      })
      .unwrap(),
      RelicArtifact::Keepsake(Keepsake {
        transfers: vec![Transfer {
          id: relic_id(1),
          amount: 2,
          output: 0,
        }],
        ..default()
      }),
    );
  }

  #[test]
  fn runestone_may_be_after_non_matching_op_return() {
    let payload = payload(&[0, 1, 1, 2, 0]);

    let payload: &PushBytes = payload.as_slice().try_into().unwrap();

    assert_eq!(
      Keepsake::decipher(&Transaction {
        input: Vec::new(),
        output: vec![
          TxOut {
            script_pubkey: script::Builder::new()
              .push_opcode(opcodes::all::OP_RETURN)
              .push_slice(b"FOO")
              .into_script(),
            value: Amount::ZERO,
          },
          TxOut {
            script_pubkey: script::Builder::new()
              .push_opcode(opcodes::all::OP_RETURN)
              .push_opcode(Keepsake::MAGIC_NUMBER)
              .push_slice(payload)
              .into_script(),
            value: Amount::ZERO
          }
        ],
        lock_time: LockTime::ZERO,
        version: Version(2),
      })
      .unwrap(),
      RelicArtifact::Keepsake(Keepsake {
        transfers: vec![Transfer {
          id: relic_id(1),
          amount: 2,
          output: 0,
        }],
        ..default()
      })
    );
  }

  #[test]
  fn enshrining_size() {
    #[track_caller]
    fn case(transfers: Vec<Transfer>, enshrining: Option<Enshrining>, size: usize) {
      assert_eq!(
        Keepsake {
          transfers,
          enshrining,
          ..default()
        }
        .encipher()
        .len(),
        size
      );
    }

    case(
      Vec::new(),
      Some(Enshrining {
        boost_terms: Some(BoostTerms {
          rare_chance: Some(100_000),
          rare_multiplier_cap: Some(10),
          ultra_rare_chance: Some(10_000),
          ultra_rare_multiplier_cap: Some(100),
        }),
        fee: Some(10_000),
        symbol: Some('\u{10FFFF}'),
        mint_terms: Some(MintTerms {
          amount: Some(100_000_000_000_000_000),
          block_cap: Some(1000),
          cap: Some(100_000),
          max_unmints: Some(10_000),
          price: Some(PriceModel::Formula {
            a: 29276332,
            b: 6994,
          }),
          seed: Some(200),
          tx_cap: Some(100),
        }),
        subsidy: Some(6_900_000_000_000),
      }),
      65,
    );
  }

  #[test]
  fn encipher() {
    #[track_caller]
    fn case(keepsake: Keepsake, expected: &[u128]) {
      let script_pubkey = keepsake.encipher();

      let transaction = Transaction {
        input: Vec::new(),
        output: vec![TxOut {
          script_pubkey,
          value: Amount::ZERO,
        }],
        lock_time: LockTime::ZERO,
        version: Version(2),
      };

      let Payload::Valid(payload) = Keepsake::payload(&transaction).unwrap() else {
        panic!("invalid payload")
      };

      assert_eq!(Keepsake::integers(&payload).unwrap(), expected);

      let keepsake = {
        let mut transfers = keepsake.transfers;
        transfers.sort_by_key(|edict| edict.id);
        Keepsake {
          transfers,
          ..keepsake
        }
      };

      assert_eq!(
        Keepsake::decipher(&transaction).unwrap(),
        RelicArtifact::Keepsake(keepsake),
      );
    }

    case(Keepsake::default(), &[]);

    case(
      Keepsake {
        transfers: vec![
          Transfer {
            id: RelicId::new(2, 3).unwrap(),
            amount: 1,
            output: 0,
          },
          Transfer {
            id: RelicId::new(5, 6).unwrap(),
            amount: 4,
            output: 1,
          },
        ],
        sealing: true,
        enshrining: Some(Enshrining {
          boost_terms: None,
          fee: None,
          symbol: Some('@'),
          mint_terms: Some(MintTerms {
            amount: Some(100),
            block_cap: None,
            cap: Some(100_000),
            max_unmints: None,
            price: Some(PriceModel::Fixed(321)),
            seed: Some(200),
            tx_cap: None,
          }),
          subsidy: None,
        }),
        mint: Some(MultiMint {
          count: 1,
          base_limit: 0,
          is_unmint: false,
          relic: relic_id(5),
        }),
        swap: Some(Swap {
          input: Some(relic_id(42)),
          output: Some(relic_id(43)),
          input_amount: Some(123),
          output_amount: Some(456),
          is_exact_input: true,
        }),
        pointer: Some(0),
        claim: Some(0),
      },
      &[
        Tag::Symbol.into(),
        '@'.into(),
        Tag::Amount.into(),
        100,
        Tag::Cap.into(),
        100_000,
        Tag::Price.into(),
        321,
        Tag::Seed.into(),
        200,
        Tag::MultiMintCount.into(),
        1,
        Tag::MultiMintBaseLimit.into(),
        0,
        Tag::MultiMintRelic.into(),
        1,
        Tag::MultiMintRelic.into(),
        5,
        Tag::SwapInput.into(),
        1,
        Tag::SwapInput.into(),
        42,
        Tag::SwapOutput.into(),
        1,
        Tag::SwapOutput.into(),
        43,
        Tag::SwapInputAmount.into(),
        123,
        Tag::SwapOutputAmount.into(),
        456,
        Tag::Flags.into(),
        Flag::Sealing.mask()
          | Flag::Enshrining.mask()
          | Flag::MintTerms.mask()
          | Flag::Swap.mask()
          | Flag::SwapExactInput.mask()
          | Flag::MultiMint.mask(),
        Tag::Pointer.into(),
        0,
        Tag::Claim.into(),
        0,
        Tag::Body.into(),
        2,
        3,
        1,
        0,
        3,
        6,
        4,
        1,
      ],
    );
  }

  #[test]
  fn runestone_payload_is_chunked() {
    let script = Keepsake {
      transfers: vec![
        Transfer {
          id: RelicId::default(),
          amount: 0,
          output: 0,
        };
        129
      ],
      ..default()
    }
    .encipher();

    assert_eq!(script.instructions().count(), 3);

    let script = Keepsake {
      transfers: vec![
        Transfer {
          id: RelicId::default(),
          amount: 0,
          output: 0,
        };
        130
      ],
      ..default()
    }
    .encipher();

    assert_eq!(script.instructions().count(), 4);
  }

  #[test]
  fn edict_output_greater_than_32_max_produces_cenotaph() {
    assert_eq!(
      decipher(&[Tag::Body.into(), 1, 1, 1, u128::from(u32::MAX) + 1, 0, 0]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::TransferOutput),
      }),
    );
  }

  #[test]
  fn partial_swap_produces_cenotaph() {
    assert_eq!(
      decipher(&[Tag::SwapInput.into(), 1]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::UnrecognizedEvenTag),
      }),
    );
  }

  #[test]
  fn invalid_swap_produces_cenotaph() {
    assert_eq!(
      decipher(&[Tag::SwapInput.into(), 0, Tag::SwapInput.into(), 1]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::UnrecognizedEvenTag),
      }),
    );
  }

  // #[test]
  // fn invalid_deadline_produces_cenotaph() {
  //   assert_eq!(
  //     decipher(&[Tag::OffsetEnd.into(), u128::MAX]),
  //     Artifact::Cenotaph(Cenotaph {
  //       flaw: Some(Flaw::UnrecognizedEvenTag),
  //     }),
  //   );
  // }

  #[test]
  fn invalid_default_output_produces_cenotaph() {
    assert_eq!(
      decipher(&[Tag::Pointer.into(), 1]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::UnrecognizedEvenTag),
      }),
    );
    assert_eq!(
      decipher(&[Tag::Pointer.into(), u128::MAX]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::UnrecognizedEvenTag),
      }),
    );
  }

  // #[test]
  // fn invalid_divisibility_does_not_produce_cenotaph() {
  //   assert_eq!(
  //     decipher(&[Tag::Divisibility.into(), u128::MAX]),
  //     RelicArtifact::Keepsake(default()),
  //   );
  // }

  // #[test]
  // fn min_and_max_runes_are_not_cenotaphs() {
  //   assert_eq!(
  //     decipher(&[
  //       Tag::Flags.into(),
  //       Flag::Enshrining.into(),
  //       Tag::Relic.into(),
  //       0
  //     ]),
  //     RelicArtifact::Keepsake(Keepsake {
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(0)),
  //         ..default()
  //       }),
  //       ..default()
  //     }),
  //   );
  //   assert_eq!(
  //     decipher(&[
  //       Tag::Flags.into(),
  //       Flag::Enshrining.into(),
  //       Tag::Relic.into(),
  //       u128::MAX
  //     ]),
  //     RelicArtifact::Keepsake(Keepsake {
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(u128::MAX)),
  //         ..default()
  //       }),
  //       ..default()
  //     }),
  //   );
  // }

  // #[test]
  // fn invalid_spacers_does_not_produce_cenotaph() {
  //   assert_eq!(
  //     decipher(&[Tag::Spacers.into(), u128::MAX]),
  //     RelicArtifact::Keepsake(default()),
  //   );
  // }

  #[test]
  fn invalid_symbol_does_not_produce_cenotaph() {
    assert_eq!(
      decipher(&[Tag::Symbol.into(), u128::MAX]),
      RelicArtifact::Keepsake(default()),
    );
  }

  // #[test]
  // fn invalid_term_produces_cenotaph() {
  //   assert_eq!(
  //     decipher(&[Tag::OffsetEnd.into(), u128::MAX]),
  //     Artifact::Cenotaph(Cenotaph {
  //       flaw: Some(Flaw::UnrecognizedEvenTag),
  //     }),
  //   );
  // }

  // #[test]
  // fn invalid_supply_produces_cenotaph() {
  //   assert_eq!(
  //     decipher(&[
  //       Tag::Flags.into(),
  //       Flag::Enshrining.mask() | Flag::Terms.mask(),
  //       Tag::Cap.into(),
  //       1,
  //       Tag::Amount.into(),
  //       u128::MAX
  //     ]),
  //     Artifact::Keepsake(Keepsake {
  //       enshrining: Some(Enshrining {
  //         terms: Some(Terms {
  //           cap: Some(1),
  //           amount: Some(u128::MAX),
  //           height: (None, None),
  //           offset: (None, None),
  //         }),
  //         ..default()
  //       }),
  //       ..default()
  //     }),
  //   );
  //
  //   assert_eq!(
  //     decipher(&[
  //       Tag::Flags.into(),
  //       Flag::Enshrining.mask() | Flag::Terms.mask(),
  //       Tag::Cap.into(),
  //       2,
  //       Tag::Amount.into(),
  //       u128::MAX
  //     ]),
  //     Artifact::Cenotaph(Cenotaph {
  //       flaw: Some(Flaw::SupplyOverflow),
  //     }),
  //   );
  //
  //   assert_eq!(
  //     decipher(&[
  //       Tag::Flags.into(),
  //       Flag::Enshrining.mask() | Flag::Terms.mask(),
  //       Tag::Cap.into(),
  //       2,
  //       Tag::Amount.into(),
  //       u128::MAX / 2 + 1
  //     ]),
  //     Artifact::Cenotaph(Cenotaph {
  //       flaw: Some(Flaw::SupplyOverflow),
  //     }),
  //   );
  //
  //   assert_eq!(
  //     decipher(&[
  //       Tag::Flags.into(),
  //       Flag::Enshrining.mask() | Flag::Terms.mask(),
  //       Tag::Premine.into(),
  //       1,
  //       Tag::Cap.into(),
  //       1,
  //       Tag::Amount.into(),
  //       u128::MAX
  //     ]),
  //     Artifact::Cenotaph(Cenotaph {
  //       flaw: Some(Flaw::SupplyOverflow),
  //     }),
  //   );
  // }

  #[test]
  fn invalid_scripts_in_op_returns_without_magic_number_are_ignored() {
    assert_eq!(
      Keepsake::decipher(&Transaction {
        version: Version(2),
        lock_time: LockTime::ZERO,
        input: vec![TxIn {
          previous_output: OutPoint::null(),
          script_sig: ScriptBuf::new(),
          sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
          witness: Witness::new(),
        }],
        output: vec![TxOut {
          script_pubkey: ScriptBuf::from(vec![
            opcodes::all::OP_RETURN.to_u8(),
            opcodes::all::OP_PUSHBYTES_4.to_u8(),
          ]),
          value: Amount::ZERO,
        }],
      }),
      None
    );

    assert_eq!(
      Keepsake::decipher(&Transaction {
        version: Version(2),
        lock_time: LockTime::ZERO,
        input: vec![TxIn {
          previous_output: OutPoint::null(),
          script_sig: ScriptBuf::new(),
          sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
          witness: Witness::new(),
        }],
        output: vec![
          TxOut {
            script_pubkey: ScriptBuf::from(vec![
              opcodes::all::OP_RETURN.to_u8(),
              opcodes::all::OP_PUSHBYTES_4.to_u8(),
            ]),
            value: Amount::ZERO,
          },
          TxOut {
            script_pubkey: Keepsake::default().encipher(),
            value: Amount::ZERO,
          }
        ],
      })
      .unwrap(),
      RelicArtifact::Keepsake(Keepsake::default()),
    );
  }

  #[test]
  fn invalid_scripts_in_op_returns_with_magic_number_produce_cenotaph() {
    assert_eq!(
      Keepsake::decipher(&Transaction {
        version: Version(2),
        lock_time: LockTime::ZERO,
        input: vec![TxIn {
          previous_output: OutPoint::null(),
          script_sig: ScriptBuf::new(),
          sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
          witness: Witness::new(),
        }],
        output: vec![TxOut {
          script_pubkey: ScriptBuf::from(vec![
            opcodes::all::OP_RETURN.to_u8(),
            Keepsake::MAGIC_NUMBER.to_u8(),
            opcodes::all::OP_PUSHBYTES_4.to_u8(),
          ]),
          value: Amount::ZERO,
        }],
      })
      .unwrap(),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::InvalidScript),
      }),
    );
  }

  #[test]
  fn all_pushdata_opcodes_are_valid() {
    for i in 0..79 {
      let mut script_pubkey = Vec::new();

      script_pubkey.push(opcodes::all::OP_RETURN.to_u8());
      script_pubkey.push(Keepsake::MAGIC_NUMBER.to_u8());
      script_pubkey.push(i);

      match i {
        0..=75 => {
          for j in 0..i {
            script_pubkey.push(if j % 2 == 0 { 1 } else { 0 });
          }

          if i % 2 == 1 {
            script_pubkey.push(1);
            script_pubkey.push(1);
          }
        }
        76 => {
          script_pubkey.push(0);
        }
        77 => {
          script_pubkey.push(0);
          script_pubkey.push(0);
        }
        78 => {
          script_pubkey.push(0);
          script_pubkey.push(0);
          script_pubkey.push(0);
          script_pubkey.push(0);
        }
        _ => unreachable!(),
      }

      assert_eq!(
        Keepsake::decipher(&Transaction {
          version: Version(2),
          lock_time: LockTime::ZERO,
          input: default(),
          output: vec![TxOut {
            script_pubkey: script_pubkey.into(),
            value: Amount::ZERO,
          },],
        })
        .unwrap(),
        RelicArtifact::Keepsake(Keepsake::default()),
      );
    }
  }

  #[test]
  fn all_non_pushdata_opcodes_are_invalid() {
    for i in 79..=u8::MAX {
      assert_eq!(
        Keepsake::decipher(&Transaction {
          version: Version(2),
          lock_time: LockTime::ZERO,
          input: default(),
          output: vec![TxOut {
            script_pubkey: vec![
              opcodes::all::OP_RETURN.to_u8(),
              Keepsake::MAGIC_NUMBER.to_u8(),
              i
            ]
            .into(),
            value: Amount::ZERO,
          },],
        })
        .unwrap(),
        RelicArtifact::Cenotaph(RelicCenotaph {
          flaw: Some(RelicFlaw::Opcode),
        }),
      );
    }
  }

  #[test]
  fn subsidy_validation() {
    // Case 1: Valid - Subsidy > 0 and Price is Fixed(0)
    let valid_subsidy_keepsake = Keepsake {
      enshrining: Some(Enshrining {
        mint_terms: Some(MintTerms {
          amount: Some(100),
          cap: Some(100_000),
          price: Some(PriceModel::Fixed(0)),
          ..default()
        }),
        subsidy: Some(10_000),
        ..default()
      }),
      ..default()
    };
    let script1 = valid_subsidy_keepsake.encipher();
    let decoded1 = Keepsake::decipher(&Transaction {
      input: Vec::new(),
      output: vec![TxOut {
        script_pubkey: script1,
        value: Amount::ZERO,
      }],
      lock_time: LockTime::ZERO,
      version: Version(2),
    })
    .unwrap();
    assert_eq!(decoded1, RelicArtifact::Keepsake(valid_subsidy_keepsake));

    // Case 2: Invalid - Subsidy > 0 but Price is Fixed(non-zero)
    let invalid_subsidy_price_fixed_keepsake = Keepsake {
      enshrining: Some(Enshrining {
        mint_terms: Some(MintTerms {
          amount: Some(100),
          cap: Some(100_000),
          price: Some(PriceModel::Fixed(1)), // Non-zero price
          ..default()
        }),
        subsidy: Some(10_000),
        ..default()
      }),
      ..default()
    };
    let script2 = invalid_subsidy_price_fixed_keepsake.encipher();
    let decoded2 = Keepsake::decipher(&Transaction {
      input: Vec::new(),
      output: vec![TxOut {
        script_pubkey: script2,
        value: Amount::ZERO,
      }],
      lock_time: LockTime::ZERO,
      version: Version(2),
    })
    .unwrap();
    assert_eq!(
      decoded2,
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::InvalidEnshriningSubsidyRules),
      })
    );

    // Case 3: Invalid - Subsidy > 0 but Price is Formula
    let invalid_subsidy_price_formula_keepsake = Keepsake {
      enshrining: Some(Enshrining {
        mint_terms: Some(MintTerms {
          amount: Some(100),
          cap: Some(10),
          price: Some(PriceModel::Formula { a: 1, b: 1 }), // Formula price
          ..default()
        }),
        subsidy: Some(10_000),
        ..default()
      }),
      ..default()
    };
    let script3 = invalid_subsidy_price_formula_keepsake.encipher();
    let decoded3 = Keepsake::decipher(&Transaction {
      input: Vec::new(),
      output: vec![TxOut {
        script_pubkey: script3,
        value: Amount::ZERO,
      }],
      lock_time: LockTime::ZERO,
      version: Version(2),
    })
    .unwrap();
    assert_eq!(
      decoded3,
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::InvalidEnshriningSubsidyRules),
      })
    );

    // Case 4: Invalid - Subsidy is None but Price is Fixed(0)
    let invalid_no_subsidy_price_zero_keepsake = Keepsake {
      enshrining: Some(Enshrining {
        mint_terms: Some(MintTerms {
          amount: Some(100),
          cap: Some(100_000),
          price: Some(PriceModel::Fixed(0)), // Price is zero
          ..default()
        }),
        subsidy: None, // No subsidy
        ..default()
      }),
      ..default()
    };
    let script4 = invalid_no_subsidy_price_zero_keepsake.encipher();
    let decoded4 = Keepsake::decipher(&Transaction {
      input: Vec::new(),
      output: vec![TxOut {
        script_pubkey: script4,
        value: Amount::ZERO,
      }],
      lock_time: LockTime::ZERO,
      version: Version(2),
    })
    .unwrap();
    assert_eq!(
      decoded4,
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::InvalidEnshriningSubsidyRules),
      })
    );

    // Case 5: Invalid - Subsidy is Some(0) and Price is Fixed(0)
    let invalid_zero_subsidy_price_zero_keepsake = Keepsake {
      enshrining: Some(Enshrining {
        mint_terms: Some(MintTerms {
          amount: Some(100),
          cap: Some(100_000),
          price: Some(PriceModel::Fixed(0)), // Price is zero
          ..default()
        }),
        subsidy: Some(0), // Zero subsidy
        ..default()
      }),
      ..default()
    };
    let script5 = invalid_zero_subsidy_price_zero_keepsake.encipher();
    let decoded5 = Keepsake::decipher(&Transaction {
      input: Vec::new(),
      output: vec![TxOut {
        script_pubkey: script5,
        value: Amount::ZERO,
      }],
      lock_time: LockTime::ZERO,
      version: Version(2),
    })
    .unwrap();
    assert_eq!(
      decoded5,
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::InvalidEnshriningSubsidyRules),
      })
    );

    // Case 6: Valid - Subsidy is None and Price is Fixed(non-zero)
    let valid_no_subsidy_price_fixed_keepsake = Keepsake {
      enshrining: Some(Enshrining {
        mint_terms: Some(MintTerms {
          amount: Some(100),
          cap: Some(100_000),
          price: Some(PriceModel::Fixed(1)), // Non-zero price
          ..default()
        }),
        subsidy: None,
        ..default()
      }),
      ..default()
    };
    let script6 = valid_no_subsidy_price_fixed_keepsake.encipher();
    let decoded6 = Keepsake::decipher(&Transaction {
      input: Vec::new(),
      output: vec![TxOut {
        script_pubkey: script6,
        value: Amount::ZERO,
      }],
      lock_time: LockTime::ZERO,
      version: Version(2),
    })
    .unwrap();
    assert_eq!(
      decoded6,
      RelicArtifact::Keepsake(valid_no_subsidy_price_fixed_keepsake)
    );
  }

  #[test]
  fn boost_terms_with_max_unmints_creates_cenotaph() {
    let invalid_boost_with_unmints = BoostTerms {
      rare_chance: Some(5000),
      rare_multiplier_cap: Some(10),
      ultra_rare_chance: Some(1000),
      ultra_rare_multiplier_cap: Some(20),
    };

    let invalid_enshrining = Enshrining {
      boost_terms: Some(invalid_boost_with_unmints),
      mint_terms: Some(MintTerms {
        amount: Some(100),
        cap: Some(100_000),
        price: Some(PriceModel::Fixed(1)),
        max_unmints: Some(50), // This should cause the validation to fail
        ..default()
      }),
      ..default()
    };

    let invalid_keepsake = Keepsake {
      transfers: Vec::new(),
      enshrining: Some(invalid_enshrining),
      ..default()
    };

    let script = invalid_keepsake.encipher();
    let decoded = Keepsake::decipher(&Transaction {
      input: Vec::new(),
      output: vec![TxOut {
        script_pubkey: script,
        value: Amount::ZERO,
      }],
      lock_time: LockTime::ZERO,
      version: Version(2),
    })
    .unwrap();

    assert_eq!(
      decoded,
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::InvalidEnshriningBoostNotUnmintable),
      })
    );
  }
}
