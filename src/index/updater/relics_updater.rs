use super::*;
use crate::index::updater::relics_balance::RelicsBalance;
use std::hash::{DefaultHasher, Hash, Hasher};

pub(super) struct RelicUpdater<'a, 'tx, 'client, 'emitter> {
  pub(super) block_time: u32,
  pub(super) burned: HashMap<RelicId, Lot>,
  pub(super) claimable: HashMap<RelicOwner, u128>,
  pub(super) unsafe_txids: HashSet<Txid>,
  pub(super) index: &'client Index,
  pub(super) height: u32,
  pub(super) id_to_entry: &'a mut Table<'tx, RelicIdValue, RelicEntryValue>,
  pub(super) inscription_id_to_sequence_number: &'a Table<'tx, InscriptionIdValue, u32>,
  pub(super) mints_in_block: HashMap<RelicId, u32>,
  pub(super) outpoint_to_balances: &'a mut Table<'tx, &'static OutPointValue, &'static [u8]>,
  pub(super) relic_owner_to_claimable: &'a mut Table<'tx, &'static RelicOwnerValue, u128>,
  pub(super) relic_to_id: &'a mut Table<'tx, u128, RelicIdValue>,
  pub(super) relics: u64,
  pub(super) statistic_to_count: &'a mut Table<'tx, u64, u64>,
  pub(super) transaction_id_to_relic: &'a mut Table<'tx, &'static TxidValue, u128>,
  pub(super) utxo_cache: &'a HashMap<OutPoint, UtxoEntryBuf>,
  pub(super) sequence_number_to_inscription_entry: &'a Table<'tx, u32, InscriptionEntryValue>,
  pub(super) sequence_number_to_satpoint: &'a Table<'tx, u32, &'static SatPointValue>,
  pub(super) sequence_number_to_spaced_relic: &'a mut Table<'tx, u32, SpacedRelicValue>,
  pub(super) relic_to_sequence_number: &'a mut Table<'tx, u128, u32>,
  pub(super) event_emitter: &'a mut EventEmitter<'emitter, 'tx>,
}

impl RelicUpdater<'_, '_, '_, '_> {
  pub(super) fn index_relics(&mut self, tx_index: u32, tx: &Transaction, txid: Txid) -> Result<()> {
    let artifact = Keepsake::decipher(tx);

    let mut balances = RelicsBalance::new(
      tx,
      &self.unsafe_txids,
      self.outpoint_to_balances,
      self.index,
    )?;

    if let Some(amount) = self.mint_base_token(txid, tx)? {
      balances.add_safe(RELIC_ID, amount);
    }

    if let Some(RelicArtifact::Keepsake(keepsake)) = &artifact {
      if keepsake.sealing {
        match self.seal(tx, txid, balances.get(RELIC_ID))? {
          Ok(sealing_fee) => {
            // burn sealing fee in RELIC
            balances.remove(RELIC_ID, sealing_fee);
            balances.burn(RELIC_ID, sealing_fee);
          }
          Err(error) => {
            eprintln!("Sealing error: {error}");
            self.event_emitter.emit(
              txid,
              EventInfo::RelicError {
                operation: RelicOperation::Seal,
                error,
              },
            )?;
          }
        }
      }

      let enshrined_relic = if let Some(enshrining_data) = keepsake.enshrining {
        match self.process_enshrinement(
          tx,
          txid,
          tx_index,
          enshrining_data,
          balances.get(RELIC_ID),
        )? {
          Ok((id, subsidy_lot)) => {
            if subsidy_lot.0 > 0 {
              balances.remove(RELIC_ID, subsidy_lot);
            }
            Some(id)
          }
          Err(error) => {
            eprintln!("Enshrine error: {error}");
            self.event_emitter.emit(
              txid,
              EventInfo::RelicError {
                operation: RelicOperation::Enshrine,
                error,
              },
            )?;
            None
          }
        }
      } else {
        None
      };

      if let Some(swap) = &keepsake.swap {
        let input = swap.input.unwrap_or(RELIC_ID);
        let output = swap.output.unwrap_or(RELIC_ID);
        // note: use safe balance here for Sandwich protection:
        // this will prevent swapping the same Relics twice within a block
        match self.swap(txid, swap, input, output, balances.get_safe(input))? {
          Ok((input_amount, output_amount, fees)) => {
            balances.remove_safe(input, Lot(input_amount));
            balances.add(output, Lot(output_amount));
            for (owner, fee) in fees {
              if let Some(owner) = owner {
                // add fees to the claimable amount of the owner
                *self.claimable.entry(owner).or_default() += fee;
              } else {
                // burn fees if there is no owner
                balances.burn(RELIC_ID, Lot(fee));
              }
            }
          }
          Err(error) => {
            eprintln!("Swap error: {error}");
            self.event_emitter.emit(
              txid,
              EventInfo::RelicError {
                operation: RelicOperation::Swap,
                error,
              },
            )?;
          }
        }
      }

      if let Some(multi) = keepsake.mint {
        // Use enshrined relic if multi.relic is default
        let id = if multi.relic == RelicId::default() {
          enshrined_relic
        } else {
          Some(multi.relic)
        };
        if let Some(id) = id {
          if multi.is_unmint {
            // Unmint not allowed if an enshrined relic is present
            if enshrined_relic.is_some() {
              eprintln!("Unmint error: Unmint not allowed in transaction with enshrined relic");
              self.event_emitter.emit(
                txid,
                EventInfo::RelicError {
                  operation: RelicOperation::Unmint,
                  error: RelicError::UnmintNotAllowed,
                },
              )?;
            } else {
              match self.unmint(txid, id, balances.get(id), multi.count, multi.base_limit)? {
                Ok(lots) => {
                  let (total_relic, total_base) = lots.iter().fold(
                    (0u128, 0u128),
                    |(acc_r, acc_b), (Lot(amount), Lot(price))| (acc_r + amount, acc_b + price),
                  );
                  self.event_emitter.emit(
                    txid,
                    EventInfo::RelicMultiMinted {
                      relic_id: id,
                      amount: total_base,
                      num_mints: multi.count,
                      base_limit: multi.base_limit,
                      is_unmint: true,
                    },
                  )?;
                  // Remove the unminted tokens from `id`'s balance and refund base tokens to RELIC balance.
                  balances.remove(id, Lot(total_relic));
                  balances.add(RELIC_ID, Lot(total_base));
                }
                Err(error) => {
                  eprintln!("MultiUnmint error: {error}");
                  self.event_emitter.emit(
                    txid,
                    EventInfo::RelicError {
                      operation: RelicOperation::MultiUnmint,
                      error,
                    },
                  )?;
                }
              }
            }
          } else {
            // Mint operation
            match self.mint(
              txid,
              id,
              balances.get(RELIC_ID),
              multi.count,
              multi.base_limit,
            )? {
              Ok(lots) => {
                let (total_relic, total_base) = lots.iter().fold(
                  (0u128, 0u128),
                  |(acc_r, acc_b), (Lot(amount), Lot(price))| (acc_r + amount, acc_b + price),
                );
                self.event_emitter.emit(
                  txid,
                  EventInfo::RelicMultiMinted {
                    relic_id: id,
                    amount: total_relic,
                    num_mints: multi.count,
                    base_limit: multi.base_limit,
                    is_unmint: false,
                  },
                )?;
                balances.remove(RELIC_ID, Lot(total_base));
                balances.add(id, Lot(total_relic));
              }
              Err(error) => {
                eprintln!("MultiMint error: {error}");
                self.event_emitter.emit(
                  txid,
                  EventInfo::RelicError {
                    operation: RelicOperation::MultiMint,
                    error,
                  },
                )?;
              }
            }
          }
        }
      }

      if let Some(claim) = keepsake.claim {
        let claim = usize::try_from(claim).unwrap();
        // values greater than the number of outputs should never be produced by the parser
        assert!(claim < tx.output.len());
        let owner = RelicOwner(tx.output[claim].script_pubkey.script_hash());
        if let Some(amount) = self.claim(txid, owner)? {
          // handle fee collection: assign all fees claimable by the given owner
          balances.allocate(claim, RELIC_ID, amount);
        } else {
          eprintln!("Claim error: no balance to claim");
          self.event_emitter.emit(
            txid,
            EventInfo::RelicError {
              operation: RelicOperation::Claim,
              error: RelicError::NoClaimableBalance,
            },
          )?;
        }
      }

      balances.allocate_transfers(&keepsake.transfers, enshrined_relic, tx);
    }

    let first_non_op_return_output = || {
      tx.output
        .iter()
        .enumerate()
        .find(|(_vout, tx_out)| !tx_out.script_pubkey.is_op_return())
        .map(|(vout, _tx_out)| vout)
    };

    let default_output = match artifact {
      // no protocol message: pass through to first non-op_return
      None => first_non_op_return_output(),
      // valid protocol message: use pointer as output or default to the first non-op_return
      Some(RelicArtifact::Keepsake(keepsake)) => keepsake
        .pointer
        .map(|pointer| pointer as usize)
        .or_else(first_non_op_return_output),
      // invalid protocol message: explicitly burn all Relics
      Some(RelicArtifact::Cenotaph(_)) => {
        eprintln!("Cenotaph encountered in tx {}: burning all relics", txid);
        None
      }
    };

    if let Some(vout) = default_output {
      // note: vout might still point to an OP_RETURN output resulting in a burn on finalize
      balances.allocate_all(vout);
    } else {
      balances.burn_all();
    }

    balances.finalize(
      tx,
      txid,
      self.outpoint_to_balances,
      &mut self.unsafe_txids,
      &mut self.burned,
      self.event_emitter,
      self.index,
    )
  }

  pub(super) fn update(self) -> Result {
    // update burned counters
    for (relic_id, burned) in self.burned {
      let mut entry = RelicEntry::load(self.id_to_entry.get(&relic_id.store())?.unwrap().value());
      entry.state.burned = entry.state.burned.checked_add(burned.n()).unwrap();
      self.id_to_entry.insert(&relic_id.store(), entry.store())?;
    }

    // update amounts of claimable balance
    for (owner, amount) in self.claimable {
      let current = self
        .relic_owner_to_claimable
        .get(&owner.store())?
        .map(|v| v.value())
        .unwrap_or_default();
      self
        .relic_owner_to_claimable
        .insert(&owner.store(), current.checked_add(amount).unwrap())?;
    }

    Ok(())
  }

  fn create_relic_entry(
    &mut self,
    txid: Txid,
    enshrining: Enshrining,
    id: RelicId,
    spaced_relic: SpacedRelic,
    owner_sequence_number: u32,
    inscription_id: InscriptionId,
  ) -> Result {
    let Enshrining {
      fee,
      symbol,
      boost_terms,
      mint_terms,
      subsidy,
    } = enshrining;

    self
      .relic_to_id
      .insert(spaced_relic.relic.store(), id.store())?;
    self
      .transaction_id_to_relic
      .insert(&txid.store(), spaced_relic.relic.store())?;

    let number = self.relics;
    self.relics += 1;

    self
      .statistic_to_count
      .insert(&Statistic::Relics.into(), self.relics)?;

    let mut entry = RelicEntry {
      block: id.block,
      enshrining: txid,
      fee: fee.unwrap_or_default(),
      number,
      spaced_relic,
      symbol,
      owner_sequence_number: Some(owner_sequence_number),
      boost_terms,
      mint_terms,
      state: RelicState {
        burned: 0,
        mints: 0,
        unmints: 0,
      },
      pool: None,
      timestamp: self.block_time.into(),
    };

    // Create pool if there's a subsidy
    if let Some(subsidy_amount) = subsidy {
      if subsidy_amount > 0 {
        entry.pool = Some(Pool {
          base_supply: 0,
          quote_supply: 0,
          fee_bps: entry.fee.min(1_000),
          subsidy: subsidy_amount,
        });
      }
    }

    self.id_to_entry.insert(&id.store(), entry.store())?;

    self.event_emitter.emit(
      txid,
      EventInfo::RelicEnshrined {
        relic_id: id,
        inscription_id,
      },
    )?;

    Ok(())
  }

  fn load_relic_entry(&self, id: RelicId) -> Result<Option<RelicEntry>> {
    let Some(entry) = self.id_to_entry.get(&id.store())? else {
      return Ok(None);
    };
    Ok(Some(RelicEntry::load(entry.value())))
  }

  fn tx_inscriptions(&self, txid: Txid, tx: &Transaction) -> Result<Vec<InscriptionEntry>> {
    let mut inscriptions: Vec<InscriptionEntry> = Vec::new();
    // we search the outputs, not the inputs, because the InscriptionUpdater has already processed
    // this transaction and would have moved any Inscription to the outputs
    for vout in 0..tx.output.len() {
      let outpoint = OutPoint {
        txid,
        vout: u32::try_from(vout).unwrap(),
      };
      let Some(utxo_entry) = self.utxo_cache.get(&outpoint) else {
        continue;
      };
      for (sequence_number, _) in utxo_entry.parse(self.index).parse_inscriptions() {
        let entry = self
          .sequence_number_to_inscription_entry
          .get(sequence_number)?
          .unwrap();
        inscriptions.push(InscriptionEntry::load(entry.value()));
      }
    }
    Ok(inscriptions)
  }

  fn seal(
    &mut self,
    tx: &Transaction,
    txid: Txid,
    base_balance: u128,
  ) -> Result<Result<Lot, RelicError>> {
    // the sealing inscription must be revealed as the first inscription in this transaction
    let inscription_id = InscriptionId { txid, index: 0 };
    let Some(sequence_number) = self
      .inscription_id_to_sequence_number
      .get(&inscription_id.store())?
      .map(|s| s.value())
    else {
      return Ok(Err(RelicError::InscriptionMissing));
    };
    let Some(inscription) = ParsedEnvelope::from_transaction(tx)
      .into_iter()
      .nth(inscription_id.index as usize)
      .map(|envelope| envelope.payload)
    else {
      panic!("failed to get Inscription envelope: {}", txid);
    };
    // parse and verify Ticker from inscribed metadata
    let Some(metadata) = inscription.metadata() else {
      // missing metadata
      return Ok(Err(RelicError::InscriptionMetadataMissing));
    };
    let Some(spaced_relic) = SpacedRelic::from_metadata(metadata) else {
      // invalid metadata
      return Ok(Err(RelicError::InvalidMetadata));
    };
    if spaced_relic == SpacedRelic::from_str(RELIC_NAME)? {
      return Ok(Err(RelicError::SealingBaseToken));
    }
    if let Some(_existing) = self.relic_to_sequence_number.get(spaced_relic.relic.n())? {
      // Ticker already sealed to an inscription
      return Ok(Err(RelicError::SealingAlreadyExists(spaced_relic)));
    }
    let sealing_fee = spaced_relic.relic.sealing_fee();
    if base_balance < sealing_fee {
      // insufficient RELIC to cover sealing fee
      return Ok(Err(RelicError::SealingInsufficientBalance(sealing_fee)));
    }
    self
      .relic_to_sequence_number
      .insert(spaced_relic.relic.n(), sequence_number)?;
    self
      .sequence_number_to_spaced_relic
      .insert(sequence_number, &spaced_relic.store())?;
    self.event_emitter.emit(
      txid,
      EventInfo::RelicSealed {
        spaced_relic,
        sequence_number,
        inscription_id,
      },
    )?;
    Ok(Ok(Lot(sealing_fee)))
  }

  fn process_enshrinement(
    &mut self,
    tx: &Transaction,
    txid: Txid,
    tx_index: u32,
    enshrining: Enshrining,
    base_balance: u128,
  ) -> Result<Result<(RelicId, Lot), RelicError>> {
    let subsidy_amount = enshrining.subsidy.unwrap_or(0);

    if subsidy_amount > 0 && base_balance < subsidy_amount {
      // Ensure RelicError::EnshrineInsufficientBalanceForSubsidy exists
      return Ok(Err(RelicError::MissingSubsidy(subsidy_amount)));
    }

    match self.enshrine_relic(tx, txid, tx_index, enshrining)? {
      Ok(relic_id) => Ok(Ok((relic_id, Lot(subsidy_amount)))),
      Err(error) => Ok(Err(error)),
    }
  }

  fn enshrine_relic(
    &mut self,
    tx: &Transaction,
    txid: Txid,
    tx_index: u32,
    enshrining: Enshrining,
  ) -> Result<Result<RelicId, RelicError>> {
    // Find all inscriptions on the outputs
    let inscriptions = self.tx_inscriptions(txid, tx)?;
    if inscriptions.is_empty() {
      return Ok(Err(RelicError::InscriptionMissing));
    }
    // Iterate through all inscriptions to find a sealed relic
    let mut spaced_relic = None;
    let mut inscription_id_for_event: Option<InscriptionId> = None;
    for entry in inscriptions {
      if let Some(relic) = self
        .sequence_number_to_spaced_relic
        .get(entry.sequence_number)?
        .map(|spaced_relic_value| SpacedRelic::load(spaced_relic_value.value()))
      {
        spaced_relic = Some((relic, entry.sequence_number));
        inscription_id_for_event = Some(entry.id);
        break; // Stop as soon as a sealed relic was found
      }
    }
    // Handle the case where no sealed relic was found
    let Some((spaced_relic, sequence_number)) = spaced_relic else {
      return Ok(Err(RelicError::SealingNotFound));
    };
    let inscription_id = inscription_id_for_event.ok_or(RelicError::InscriptionMissing)?;

    // Bail out if Relic ticker is already enshrined
    if self.relic_to_id.get(spaced_relic.relic.n())?.is_some() {
      return Ok(Err(RelicError::RelicAlreadyEnshrined));
    }

    // Create a new RelicId and enshrine the relic
    let id = RelicId {
      block: self.height.into(),
      tx: tx_index,
    };
    self.create_relic_entry(
      txid,
      enshrining,
      id,
      spaced_relic,
      sequence_number,
      inscription_id,
    )?;
    Ok(Ok(id))
  }

  fn swap(
    &mut self,
    txid: Txid,
    swap: &Swap,
    input: RelicId,
    output: RelicId,
    input_balance: u128,
  ) -> Result<Result<(u128, u128, Vec<(Option<RelicOwner>, u128)>), RelicError>> {
    assert_ne!(
      input, output,
      "the parser produced an invalid Swap with input Relic == output Relic"
    );
    let input_entry = self.load_relic_entry(input)?;
    let output_entry = self.load_relic_entry(output)?;
    match self.swap_calculate(
      swap,
      input,
      &input_entry,
      output,
      &output_entry,
      input_balance,
    ) {
      Ok((sell, buy)) => {
        let mut fees = Vec::new();
        if let Some(diff) = sell {
          fees.push(self.swap_apply(swap, txid, input, &mut input_entry.unwrap(), diff)?);
        }
        if let Some(diff) = buy {
          fees.push(self.swap_apply(swap, txid, output, &mut output_entry.unwrap(), diff)?);
        }
        match (sell, buy) {
          (Some(sell), None) => Ok(Ok((sell.input, sell.output, fees))),
          (None, Some(buy)) => Ok(Ok((buy.input, buy.output, fees))),
          (Some(sell), Some(buy)) => Ok(Ok((sell.input, buy.output, fees))),
          (None, None) => unreachable!(),
        }
      }
      Err(cause) => Ok(Err(cause)),
    }
  }

  fn swap_calculate(
    &self,
    swap: &Swap,
    input: RelicId,
    input_entry: &Option<RelicEntry>,
    output: RelicId,
    output_entry: &Option<RelicEntry>,
    input_balance: u128,
  ) -> Result<(Option<BalanceDiff>, Option<BalanceDiff>), RelicError> {
    let simple_swap = |direction: SwapDirection| {
      if swap.is_exact_input {
        PoolSwap::Input {
          direction,
          input: swap.input_amount.unwrap_or_default(),
          min_output: swap.output_amount,
        }
      } else {
        PoolSwap::Output {
          direction,
          output: swap.output_amount.unwrap_or_default(),
          max_input: swap.input_amount,
        }
      }
    };
    let input_entry = input_entry.ok_or(RelicError::RelicNotFound(input))?;
    let output_entry = output_entry.ok_or(RelicError::RelicNotFound(output))?;
    match (input, output) {
      // buy output relic
      (RELIC_ID, _) => Ok((
        None,
        Some(output_entry.swap(simple_swap(SwapDirection::BaseToQuote), Some(input_balance))?),
      )),
      // sell input relic
      (_, RELIC_ID) => Ok((
        Some(input_entry.swap(simple_swap(SwapDirection::QuoteToBase), Some(input_balance))?),
        None,
      )),
      // dual swap: sell input relic to buy output relic
      _ => {
        if swap.is_exact_input {
          // sell input
          let diff_sell = input_entry.swap(
            PoolSwap::Input {
              direction: SwapDirection::QuoteToBase,
              input: swap.input_amount.unwrap_or_default(),
              // no slippage check here, we check on the other swap
              min_output: None,
            },
            Some(input_balance),
          )?;
          // buy output
          let diff_buy = output_entry.swap(
            PoolSwap::Input {
              direction: SwapDirection::BaseToQuote,
              input: diff_sell.output,
              // slippage check is performed on the second swap, on slippage error both swaps will not be executed
              min_output: swap.output_amount,
            },
            None,
          )?;
          Ok((Some(diff_sell), Some(diff_buy)))
        } else {
          // calculate the "buy" first to determine how many base tokens we need to get out of the "sell"
          let diff_buy = output_entry.swap(
            PoolSwap::Output {
              direction: SwapDirection::BaseToQuote,
              output: swap.output_amount.unwrap_or_default(),
              // no slippage check here, we check on the other swap
              max_input: None,
            },
            None,
          )?;
          // sell input
          let diff_sell = input_entry.swap(
            PoolSwap::Output {
              direction: SwapDirection::QuoteToBase,
              output: diff_buy.input,
              // slippage check is performed on the second swap, on slippage error both swaps will not be executed
              max_input: swap.input_amount,
            },
            Some(input_balance),
          )?;
          Ok((Some(diff_sell), Some(diff_buy)))
        }
      }
    }
  }

  fn get_inscription_owner(&self, sequence_number: u32) -> Result<Option<RelicOwner>> {
    // check utxo cache first
    for utxo_entry in self.utxo_cache.values() {
      let utxo_entry = utxo_entry.parse(self.index);
      for (seq, _) in utxo_entry.parse_inscriptions() {
        if seq == sequence_number {
          let script = Script::from_bytes(utxo_entry.script_pubkey());
          return Ok(Some(RelicOwner(script.script_hash())));
        }
      }
    }
    // on cache-miss check database
    let Some(satpoint) = self
      .sequence_number_to_satpoint
      .get(sequence_number)?
      .map(|satpoint| SatPoint::load(*satpoint.value()))
    else {
      panic!("unable to find satpoint for sequence number {sequence_number}");
    };
    if satpoint.outpoint == unbound_outpoint() || satpoint.outpoint == OutPoint::null() {
      return Ok(None);
    }
    let Some(tx_info) = self
      .index
      .client
      .get_raw_transaction_info(&satpoint.outpoint.txid, None)
      .into_option()?
    else {
      panic!("can't get input transaction: {}", satpoint.outpoint.txid);
    };
    let script = tx_info.vout[satpoint.outpoint.vout as usize]
      .script_pub_key
      .script()?;
    Ok(Some(RelicOwner(script.script_hash())))
  }

  fn swap_apply(
    &mut self,
    swap: &Swap,
    txid: Txid,
    relic_id: RelicId,
    entry: &mut RelicEntry,
    diff: BalanceDiff,
  ) -> Result<(Option<RelicOwner>, u128)> {
    entry.pool.as_mut().unwrap().apply(diff);
    self.id_to_entry.insert(&relic_id.store(), entry.store())?;
    let owner = if diff.fee > 0 {
      if let Some(sequence_number) = entry.owner_sequence_number {
        self.get_inscription_owner(sequence_number)?
      } else {
        None
      }
    } else {
      None
    };
    let (base_amount, quote_amount, fee, is_sell_order) = match diff.direction {
      SwapDirection::BaseToQuote => (diff.input, diff.output, diff.fee, false),
      SwapDirection::QuoteToBase => (diff.output, diff.input, diff.fee, true),
    };
    self.event_emitter.emit(
      txid,
      EventInfo::RelicSwapped {
        relic_id,
        base_amount,
        quote_amount,
        fee,
        is_sell_order,
        is_exact_input: swap.is_exact_input,
      },
    )?;
    Ok((owner, diff.fee))
  }

  /// mint base token for every burned inception inscription in the tx
  fn mint_base_token(&mut self, txid: Txid, tx: &Transaction) -> Result<Option<Lot>> {
    let mut burned_inceptions = 0;
    let inscriptions = self.tx_inscriptions(txid, tx)?;
    for inscription in inscriptions {
      if !Charm::Burned.is_set(inscription.charms) {
        continue;
      };

      if self.index.settings.integration_test() {
        burned_inceptions += 1;
        continue;
      }

      for parent_seq_number in inscription.parents {
        // Normal operation (non-integration test)
        let inception_parent_seq_number = if let Ok(Some(seq_number)) = self
          .inscription_id_to_sequence_number
          .get(InscriptionId::from_str(INCEPTION_PARENT_INSCRIPTION_ID)?.store())
        {
          seq_number
        } else {
          continue;
        };
        if parent_seq_number == inception_parent_seq_number.value() {
          burned_inceptions += 1;
        }
      }
    }

    if burned_inceptions == 0 {
      return Ok(None);
    }

    let mut relic = self.load_relic_entry(RELIC_ID)?.unwrap();
    let terms = relic.mint_terms.unwrap();
    assert!(
      relic.state.mints + burned_inceptions <= terms.cap.unwrap(),
      "too many mints of the base token, is the cap set correctly?"
    );
    relic.state.mints += burned_inceptions;
    let amount = terms.amount.unwrap() * burned_inceptions;

    self.id_to_entry.insert(&RELIC_ID.store(), relic.store())?;

    self.event_emitter.emit(
      txid,
      EventInfo::RelicMinted {
        relic_id: RELIC_ID,
        amount,
        multiplier: 1,
        is_unmint: false,
      },
    )?;

    Ok(Some(Lot(amount)))
  }

  fn compute_boost_multiplier(
    &self,
    relic_id: &RelicId,
    txid: &Txid,
    mint_index: u128,
    boost: &BoostTerms,
  ) -> u32 {
    let mut hasher = DefaultHasher::new();
    relic_id.block.hash(&mut hasher);
    txid.hash(&mut hasher);
    mint_index.hash(&mut hasher);
    let seed = hasher.finish();
    let rand_val = (seed % 1_000_000) as u32; // Random value between 0 and 999,999

    let mut multiplier = 1;

    if let (Some(ur_chance), Some(ur_multiplier_cap)) =
      (boost.ultra_rare_chance, boost.ultra_rare_multiplier_cap)
    {
      if rand_val < ur_chance {
        // Ultra-rare case: multiplier between rare_multiplier_cap and ultra_rare_multiplier_cap
        if let Some(r_multiplier_cap) = boost.rare_multiplier_cap {
          let min = u32::from(r_multiplier_cap);
          let max = u32::from(ur_multiplier_cap);
          let range = max - min + 1; // Inclusive range
          multiplier = min + (rand_val % range); // Random value in [min, max]
        }
      }
    }

    if multiplier == 1 {
      if let (Some(r_chance), Some(r_multiplier_cap)) =
        (boost.rare_chance, boost.rare_multiplier_cap)
      {
        if rand_val < r_chance {
          // Rare case: multiplier between 1 and rare_multiplier_cap
          let min = 1;
          let max = u32::from(r_multiplier_cap);
          let range = max - min + 1; // Inclusive range
          multiplier = min + (rand_val % range); // Random value in [min, max]
        }
      }
    }

    multiplier
  }

  fn mint(
    &mut self,
    txid: Txid,
    id: RelicId,
    base_balance: u128,
    requested_mints: u8,
    base_limit: u128,
  ) -> Result<Result<Vec<(Lot, Lot)>, RelicError>> {
    assert_ne!(
      id, RELIC_ID,
      "the parser produced an invalid mint for the base token"
    );
    let Some(mut relic_entry) = self.load_relic_entry(id)? else {
      return Ok(Err(RelicError::RelicNotFound(id)));
    };

    // Determine mintable results based on current state and balance limits
    let potential_mints = match relic_entry.mintable(base_balance, requested_mints, base_limit) {
      Ok(results) => results,
      Err(cause) => return Ok(Err(cause)),
    };

    if potential_mints.is_empty() {
      // This can happen if base_limit is too high or balance is insufficient for even one mint.
      // mintable already checks for MintCapReached and MintAmountZero.
      // We might need a more specific error if base_limit caused this.
      // For now, assume mintable returns the appropriate error or an empty Vec if validly 0 mints possible.
      return Ok(Ok(Vec::new()));
    }

    let mut num_mints_to_perform = potential_mints.len();

    // Check per-block mint limit if set.
    if let Some(terms) = relic_entry.mint_terms {
      if let Some(block_cap) = terms.block_cap {
        let current_in_block = self.mints_in_block.get(&id).cloned().unwrap_or(0);
        let remaining_in_block = block_cap.saturating_sub(current_in_block);
        if remaining_in_block == 0 {
          return Ok(Err(RelicError::MintBlockCapExceeded(block_cap)));
        }
        num_mints_to_perform = num_mints_to_perform.min(remaining_in_block as usize);
      }
    }

    if num_mints_to_perform == 0 {
      // Could be due to block cap limit after initial checks
      return Ok(Ok(Vec::new()));
    }

    let current_mints_state = relic_entry.state.mints;
    let mut final_results = Vec::with_capacity(num_mints_to_perform);
    let mut total_price = 0u128;

    // Apply boosts and calculate total price for the allowed mints
    for (i, (base_amount, price)) in potential_mints
      .iter()
      .take(num_mints_to_perform)
      .enumerate()
    {
      let mut final_amount = *base_amount;
      let mut multiplier = 1;
      if let Some(boost) = relic_entry.boost_terms {
        let mint_index = current_mints_state + i as u128;
        multiplier = self.compute_boost_multiplier(&id, &txid, mint_index, &boost);
        final_amount = final_amount
          .checked_mul(u128::from(multiplier))
          .unwrap_or(final_amount);
      }
      final_results.push((final_amount, *price, multiplier));
      total_price = total_price
        .checked_add(*price)
        .ok_or(RelicError::Unmintable)?;
    }

    // Final balance check after calculating boosts and total price
    if base_balance < total_price {
      return Ok(Err(RelicError::MintInsufficientBalance(total_price)));
    }

    // Update block mint counter
    if let Some(terms) = relic_entry.mint_terms {
      if terms.block_cap.is_some() {
        let counter = self.mints_in_block.entry(id).or_insert(0);
        *counter = counter.saturating_add(num_mints_to_perform.try_into()?);
      }
    }

    // Update relic state
    relic_entry.state.mints += num_mints_to_perform as u128;

    // Check for pool creation
    if let Some(terms) = relic_entry.mint_terms {
      if relic_entry.state.mints == terms.cap.unwrap_or_default() {
        let base_supply = relic_entry.locked_base_supply();
        let quote_supply = terms.seed.unwrap_or_default();
        let fee_bps = relic_entry.fee.min(1_000);

        // Assert that if a pool exists, it must meet the special case conditions
        if let Some(existing_pool) = &relic_entry.pool {
          assert!(
            existing_pool.subsidy > 0
              && existing_pool.base_supply == 0
              && existing_pool.quote_supply == 0
              && existing_pool.fee_bps == fee_bps,
            "existing pool must have subsidy > 0 and zero supplies and same fee"
          );
        }

        if base_supply > 0 && quote_supply > 0 {
          relic_entry.pool = Some(Pool {
            base_supply,
            quote_supply,
            fee_bps,
            // reset the subsidy, should not be taken again
            subsidy: 0,
          });
        } else {
          eprintln!(
            "unable to create pool for Relic {}: both token supplies must be non-zero, but got base/quote supply of {base_supply}/{quote_supply}",
            relic_entry.spaced_relic
          );
        }
      }
    }

    // Save the updated entry *before* emitting events
    self.id_to_entry.insert(&id.store(), relic_entry.store())?;

    // Emit events for each successful mint
    let mut lots_result = Vec::with_capacity(num_mints_to_perform);
    for (amount, price, multiplier) in final_results {
      self.event_emitter.emit(
        txid,
        EventInfo::RelicMinted {
          relic_id: id,
          amount,
          multiplier,
          is_unmint: false,
        },
      )?;
      lots_result.push((Lot(amount), Lot(price)));
    }

    Ok(Ok(lots_result))
  }

  fn unmint(
    &mut self,
    txid: Txid,
    id: RelicId,
    balance: u128,
    count: u8,
    base_min: u128, // minimum base tokens the user expects to receive
  ) -> Result<Result<Vec<(Lot, Lot)>, RelicError>> {
    assert_ne!(id, RELIC_ID, "unmint for base token is not allowed");
    let Some(mut relic_entry) = self.load_relic_entry(id)? else {
      return Ok(Err(RelicError::RelicNotFound(id)));
    };

    let results = match relic_entry.unmintable(balance, count, base_min) {
      Ok(res) => res,
      Err(e) => return Ok(Err(e)),
    };

    // Total minted tokens to be removed.
    let total_minted: u128 = results.iter().map(|(a, _)| *a).sum();

    relic_entry.state.mints -= u128::from(count);
    relic_entry.state.unmints += u128::from(count);
    self.id_to_entry.insert(&id.store(), relic_entry.store())?;
    self.event_emitter.emit(
      txid,
      EventInfo::RelicMultiMinted {
        relic_id: id,
        amount: total_minted,
        num_mints: count,
        base_limit: base_min,
        is_unmint: true,
      },
    )?;
    let lots = results.into_iter().map(|(a, p)| (Lot(a), Lot(p))).collect();
    Ok(Ok(lots))
  }

  fn claim(&mut self, txid: Txid, owner: RelicOwner) -> Result<Option<Lot>> {
    // claimable balance collected before the current block and persisted to the database
    let old = self
      .relic_owner_to_claimable
      .remove(&owner.store())?
      .map(|v| v.value());
    // claimable balance collected during indexing of the current block
    let new = self.claimable.remove(&owner);
    if old.is_none() && new.is_none() {
      return Ok(None);
    }
    let amount = Lot(old.unwrap_or_default()) + new.unwrap_or_default();
    self
      .event_emitter
      .emit(txid, EventInfo::RelicClaimed { amount: amount.n() })?;
    Ok(Some(amount))
  }
}
