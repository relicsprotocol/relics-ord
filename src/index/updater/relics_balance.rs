use super::*;

pub struct RelicsBalance {
  total: HashMap<RelicId, Lot>,
  safe: HashMap<RelicId, Lot>,
  burned: HashMap<RelicId, Lot>,
  allocated: Vec<HashMap<RelicId, Lot>>,
  incoming: HashMap<(Address, RelicId), Lot>,
  outgoing: HashMap<(Address, RelicId), Lot>,
}

impl<'a, 'tx, 'emitter> RelicsBalance {
  pub fn new(
    tx: &Transaction,
    unsafe_txids: &HashSet<Txid>,
    outpoint_to_balances: &'a mut Table<'tx, &'static OutPointValue, &'static [u8]>,
    index: &Index,
  ) -> Result<Self> {
    // map of RelicsId to unallocated balance of that Relic
    let mut total: HashMap<RelicId, Lot> = HashMap::new();
    // only counts Relics from outpoints that are not from the same block
    let mut safe: HashMap<RelicId, Lot> = HashMap::new();
    // tracks which address contributed which Relic
    let mut incoming: HashMap<(Address, RelicId), Lot> = HashMap::new();

    // increment unallocated Relics with the Relics in tx inputs
    for input in &tx.input {
      let Some(guard) = outpoint_to_balances.remove(&input.previous_output.store())? else {
        continue;
      };
      let sender = if let Some(tx) = index.get_transaction(input.previous_output.txid)? {
        let output = &tx.output[input.previous_output.vout as usize];
        index
          .settings
          .chain()
          .address_from_script(&output.script_pubkey)
          .ok()
      } else {
        None
      };
      let buffer = guard.value();
      let mut i = 0;
      while i < buffer.len() {
        let ((id, balance), len) = Index::decode_rune_balance(&buffer[i..]).unwrap();
        i += len;
        // sum up total balance
        *total.entry(id).or_default() += balance;
        // sum up safe balance
        if !unsafe_txids.contains(&input.previous_output.txid) {
          *safe.entry(id).or_default() += balance;
        }
        // track where the balances came from
        if let Some(sender) = sender.clone() {
          *incoming.entry((sender, id)).or_default() += balance;
        }
      }
    }
    Ok(RelicsBalance {
      total,
      safe,
      burned: HashMap::new(),
      allocated: vec![HashMap::new(); tx.output.len()],
      incoming,
      outgoing: HashMap::new(),
    })
  }

  fn lookup(entries: &HashMap<RelicId, Lot>, id: RelicId) -> u128 {
    entries.get(&id).map(|lot| lot.n()).unwrap_or_default()
  }

  pub fn get(&self, id: RelicId) -> u128 {
    RelicsBalance::lookup(&self.total, id)
  }

  pub fn get_safe(&self, id: RelicId) -> u128 {
    RelicsBalance::lookup(&self.safe, id)
  }

  /// This will panic if there is not enough balance.
  /// Will spent safe balance last.
  pub fn remove(&mut self, id: RelicId, amount: Lot) {
    let total = self.total.entry(id).or_default();
    let safe = self.safe.entry(id).or_default();
    *total -= amount;
    if total < safe {
      *safe = *total;
    }
  }

  /// This will panic if there is not enough *safe* balance.
  pub fn remove_safe(&mut self, id: RelicId, amount: Lot) {
    *self.total.entry(id).or_default() -= amount;
    *self.safe.entry(id).or_default() -= amount;
  }

  /// Add to total balance, will not count towards *safe* balance.
  pub fn add(&mut self, id: RelicId, amount: Lot) {
    *self.total.entry(id).or_default() += amount;
  }

  /// Add to *safe* balance.
  pub fn add_safe(&mut self, id: RelicId, amount: Lot) {
    *self.total.entry(id).or_default() += amount;
    *self.safe.entry(id).or_default() += amount;
  }

  pub fn burn(&mut self, id: RelicId, amount: Lot) {
    *self.burned.entry(id).or_default() += amount;
  }

  pub fn burn_all(&mut self) {
    for (id, balance) in self.total.clone() {
      self.burn(id, balance);
    }
    self.total.clear();
    self.safe.clear();
  }

  // Allocate all
  pub fn allocate_all(&mut self, output: usize) {
    for (id, balance) in self.total.clone() {
      self.allocate(output, id, balance);
    }
    self.total.clear();
    self.safe.clear();
  }

  /// Allocate given Relics to output.
  pub fn allocate(&mut self, output: usize, id: RelicId, amount: Lot) {
    assert!(output < self.allocated.len());
    if amount > 0 {
      *self.allocated[output].entry(id).or_default() += amount;
    }
  }

  /// Allocate Relics based on the given transfers.
  pub fn allocate_transfers(
    &mut self,
    transfers: &[Transfer],
    default: Option<RelicId>,
    tx: &Transaction,
  ) {
    // this algorithm does not handle safe balance, therefore it is just cleared
    self.safe.clear();
    for Transfer { id, amount, output } in transfers.iter().copied() {
      let amount = Lot(amount);

      // edicts with output values greater than the number of outputs
      // should never be produced by the edict parser
      let output = usize::try_from(output).unwrap();
      assert!(output <= tx.output.len());

      let id = if id == RelicId::default() {
        let Some(id) = default else {
          continue;
        };
        id
      } else {
        id
      };

      let Some(balance) = self.total.get_mut(&id) else {
        continue;
      };

      let mut allocate = |balance: &mut Lot, amount: Lot, output: usize| {
        if amount > 0 {
          *balance -= amount;
          *self.allocated[output].entry(id).or_default() += amount;
        }
      };

      if output == tx.output.len() {
        // find non-OP_RETURN outputs
        let destinations = tx
          .output
          .iter()
          .enumerate()
          .filter_map(|(output, tx_out)| (!tx_out.script_pubkey.is_op_return()).then_some(output))
          .collect::<Vec<usize>>();

        if !destinations.is_empty() {
          if amount == 0 {
            // if amount is zero, divide balance between eligible outputs
            let amount = *balance / destinations.len() as u128;
            let remainder = usize::try_from(*balance % destinations.len() as u128).unwrap();

            for (i, output) in destinations.iter().enumerate() {
              allocate(
                balance,
                if i < remainder { amount + 1 } else { amount },
                *output,
              );
            }
          } else {
            // if amount is non-zero, distribute amount to eligible outputs
            for output in destinations {
              allocate(balance, amount.min(*balance), output);
            }
          }
        }
      } else {
        // Get the allocatable amount
        let amount = if amount == 0 {
          *balance
        } else {
          amount.min(*balance)
        };

        allocate(balance, amount, output);
      }
    }
  }

  /// Assign allocated balances to outpoints, update burned balances, track unsafe outpoints.
  pub fn finalize(
    mut self,
    tx: &Transaction,
    txid: Txid,
    outpoint_to_balances: &'a mut Table<'tx, &'static OutPointValue, &'static [u8]>,
    unsafe_txids: &'a mut HashSet<Txid>,
    burned: &'a mut HashMap<RelicId, Lot>,
    event_emitter: &'a mut EventEmitter<'emitter, 'tx>,
    index: &Index,
  ) -> Result {
    // update outpoint balances
    let mut buffer: Vec<u8> = Vec::new();
    for (vout, balances) in self.allocated.into_iter().enumerate() {
      if balances.is_empty() {
        continue;
      }

      // increment burned balances
      if tx.output[vout].script_pubkey.is_op_return() {
        for (id, balance) in &balances {
          *self.burned.entry(*id).or_default() += *balance;
        }
        continue;
      }

      buffer.clear();

      let mut balances = balances.into_iter().collect::<Vec<(RelicId, Lot)>>();

      // Sort balances by id so tests can assert balances in a fixed order
      balances.sort();

      let outpoint = OutPoint {
        txid,
        vout: vout.try_into().unwrap(),
      };

      for (id, balance) in balances {
        Index::encode_rune_balance(id, balance.n(), &mut buffer);

        let output_script = &tx.output[vout].script_pubkey;
        if let Ok(receiver) = index.settings.chain().address_from_script(output_script) {
          *self.outgoing.entry((receiver, id)).or_default() += balance;
        }

        event_emitter.emit(
          txid,
          EventInfo::RelicTransferred {
            relic_id: id,
            amount: balance.n(),
            output: outpoint.vout,
          },
        )?;
      }

      outpoint_to_balances.insert(&outpoint.store(), buffer.as_slice())?;
    }

    for ((address, relic_id), spent) in self.incoming {
      let info = if let Some(received) = self.outgoing.remove(&(address.clone(), relic_id)) {
        if received > spent {
          // spent less than received => net received
          EventInfo::RelicReceived {
            relic_id,
            amount: (received - spent).n(),
            address: address.as_unchecked().clone(),
          }
        } else {
          // received less than spent => net spent
          EventInfo::RelicSpent {
            relic_id,
            amount: (spent - received).n(),
            address: address.as_unchecked().clone(),
          }
        }
      } else {
        // received none, spent all
        EventInfo::RelicSpent {
          relic_id,
          amount: spent.n(),
          address: address.as_unchecked().clone(),
        }
      };
      event_emitter.emit(txid, info)?
    }
    for ((address, id), received) in self.outgoing {
      event_emitter.emit(
        txid,
        // spent none, received all
        EventInfo::RelicReceived {
          relic_id: id,
          amount: received.n(),
          address: address.as_unchecked().clone(),
        },
      )?;
    }

    // increment entries with burned relics
    for (id, amount) in self.burned {
      *burned.entry(id).or_default() += amount;

      event_emitter.emit(
        txid,
        EventInfo::RelicBurned {
          relic_id: id,
          amount: amount.n(),
        },
      )?;
    }

    // Sandwich Protection: mark OutPoints from this Tx as unsafe
    unsafe_txids.insert(txid);

    Ok(())
  }
}
