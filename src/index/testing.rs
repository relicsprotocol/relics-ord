use relics_protocol::INCEPTION_PARENT_INSCRIPTION_ID;
use {super::*, bitcoin::script::PushBytes, std::ffi::OsString, tempfile::TempDir};

pub(crate) struct ContextBuilder {
  args: Vec<OsString>,
  chain: Chain,
  event_sender: Option<tokio::sync::mpsc::Sender<Event>>,
  tempdir: Option<TempDir>,
}

impl ContextBuilder {
  pub(crate) fn build(self) -> Context {
    self.try_build().unwrap()
  }

  pub(crate) fn try_build(self) -> Result<Context> {
    let core = mockcore::builder().network(self.chain.network()).build();

    let tempdir = self.tempdir.unwrap_or_else(|| TempDir::new().unwrap());
    let cookie_file = tempdir.path().join("cookie");
    fs::write(&cookie_file, "username:password").unwrap();

    let command: Vec<OsString> = vec![
      "ord".into(),
      "--bitcoin-rpc-url".into(),
      core.url().into(),
      "--datadir".into(),
      tempdir.path().into(),
      "--cookie-file".into(),
      cookie_file.into(),
      format!("--chain={}", self.chain).into(),
    ];

    let options = Options::try_parse_from(command.into_iter().chain(self.args)).unwrap();

    let index = Index::open_with_event_sender(
      &Settings::from_options(options).or_defaults().unwrap(),
      self.event_sender,
    )?;

    index.update().unwrap();

    Ok(Context {
      index,
      core,
      tempdir,
    })
  }

  pub(crate) fn arg(mut self, arg: impl Into<OsString>) -> Self {
    self.args.push(arg.into());
    self
  }

  pub(crate) fn args<T: Into<OsString>, I: IntoIterator<Item = T>>(mut self, args: I) -> Self {
    self.args.extend(args.into_iter().map(|arg| arg.into()));
    self
  }

  pub(crate) fn chain(mut self, chain: Chain) -> Self {
    self.chain = chain;
    self
  }

  pub(crate) fn tempdir(mut self, tempdir: TempDir) -> Self {
    self.tempdir = Some(tempdir);
    self
  }

  pub(crate) fn event_sender(mut self, sender: tokio::sync::mpsc::Sender<Event>) -> Self {
    self.event_sender = Some(sender);
    self
  }
}

pub(crate) struct Context {
  pub(crate) index: Index,
  pub(crate) core: mockcore::Handle,
  #[allow(unused)]
  pub(crate) tempdir: TempDir,
}

impl Context {
  pub(crate) fn builder() -> ContextBuilder {
    ContextBuilder {
      args: Vec::new(),
      chain: Chain::Regtest,
      event_sender: None,
      tempdir: None,
    }
  }

  #[track_caller]
  pub(crate) fn mine_blocks(&self, n: u64) -> Vec<Block> {
    self.mine_blocks_with_update(n, true)
  }

  #[track_caller]
  pub(crate) fn mine_blocks_with_update(&self, n: u64, update: bool) -> Vec<Block> {
    let blocks = self.core.mine_blocks(n);
    if update {
      self.index.update().unwrap();
    }
    blocks
  }

  pub(crate) fn mine_blocks_with_subsidy(&self, n: u64, subsidy: u64) -> Vec<Block> {
    let blocks = self.core.mine_blocks_with_subsidy(n, subsidy);
    self.index.update().unwrap();
    blocks
  }

  pub(crate) fn configurations() -> Vec<Context> {
    vec![
      Context::builder().build(),
      Context::builder().arg("--index-sats").build(),
    ]
  }

  #[track_caller]
  pub(crate) fn assert_relics(
    &self,
    mut relics: impl AsMut<[(RelicId, RelicEntry)]>,
    mut balances: impl AsMut<[(OutPoint, Vec<(RelicId, u128)>)]>,
  ) {
    let relics = relics.as_mut();
    relics.sort_by_key(|(id, _)| *id);

    let balances = balances.as_mut();
    balances.sort_by_key(|(outpoint, _)| *outpoint);

    for (_, balances) in balances.iter_mut() {
      balances.sort_by_key(|(id, _)| *id);
    }

    pretty_assert_eq!(relics, self.index.relics().unwrap());

    pretty_assert_eq!(balances, self.index.get_relic_balances().unwrap());

    let mut outstanding: HashMap<RelicId, u128> = HashMap::new();

    for (_, balances) in balances {
      for (id, balance) in balances {
        *outstanding.entry(*id).or_default() += *balance;
      }
    }

    // sum up all base tokens locked in pools or mints to correct the balance check below
    let locked_base: u128 = relics
      .iter()
      .map(|(_, entry)| entry.locked_base_supply())
      .sum();
    // sum up all unclaimed base tokens
    let claimable_base: u128 = self
      .index
      .get_relic_claimable()
      .unwrap()
      .iter()
      .map(|(_, amount)| amount)
      .sum();

    for (id, entry) in relics {
      if *id != RELIC_ID {
        continue;
      }
      let correction = locked_base + claimable_base;
      pretty_assert_eq!(
        outstanding.get(id).copied().unwrap_or_default(),
        entry.circulating_supply() - correction,
        "unexpected circulating supply for {}",
        entry.spaced_relic
      );
    }
  }

  #[track_caller]
  pub(crate) fn assert_runes(
    &self,
    mut runes: impl AsMut<[(RuneId, RuneEntry)]>,
    mut balances: impl AsMut<[(OutPoint, Vec<(RuneId, u128)>)]>,
  ) {
    let runes = runes.as_mut();
    runes.sort_by_key(|(id, _)| *id);

    let balances = balances.as_mut();
    balances.sort_by_key(|(outpoint, _)| *outpoint);

    for (_, balances) in balances.iter_mut() {
      balances.sort_by_key(|(id, _)| *id);
    }

    pretty_assert_eq!(runes, self.index.runes().unwrap());

    pretty_assert_eq!(balances, self.index.get_rune_balances().unwrap());

    let mut outstanding: HashMap<RuneId, u128> = HashMap::new();

    for (_, balances) in balances {
      for (id, balance) in balances {
        *outstanding.entry(*id).or_default() += *balance;
      }
    }

    for (id, entry) in runes {
      pretty_assert_eq!(
        outstanding.get(id).copied().unwrap_or_default(),
        entry.supply() - entry.burned
      );
    }
  }

  pub(crate) fn base_token_entry() -> RelicEntry {
    RelicEntry {
      block: 1,
      enshrining: Txid::all_zeros(),
      number: 0,
      spaced_relic: SpacedRelic {
        relic: Relic(230362),
        spacers: 0,
      },
      symbol: Some('𝕄'),
      owner_sequence_number: None,
      mint_terms: Some(MintTerms {
        amount: Some(654_205_000_000),
        cap: Some(3210),
        ..default()
      }),
      boost_terms: None,
      fee: 100,
      state: RelicState { ..default() },
      pool: None,
      timestamp: 0,
    }
  }

  pub(crate) fn mint_base_token(&self, n: u32, outputs: usize) -> (Txid, RelicEntry) {
    assert!(n > 0, "must mint at least once");
    assert!(outputs > 0, "must have at least one output");

    let block_reward = Amount::from_int_btc(50);
    let dust_value = Amount::from_sat(10000);

    let block_count = usize::try_from(self.index.block_count().unwrap()).unwrap();
    self.mine_blocks(1);
    // create new UTXO per requested mint
    let mut output_values = vec![(block_reward - dust_value * u64::from(n - 1)).to_sat()];
    for _ in 1..n {
      output_values.push(dust_value.to_sat());
    }
    self.core.broadcast_tx(TransactionTemplate {
      inputs: &[(block_count, 0, 0, Default::default())],
      outputs: n as usize,
      output_values: &output_values,
      ..default()
    });

    self.mine_blocks(1);
    let parents = vec![InscriptionId::from_str(INCEPTION_PARENT_INSCRIPTION_ID)
      .unwrap()
      .value()];

    let inception_inscription = Inscription {
      parents,
      // create all inscriptions on the same sat to simplify burning all of them
      pointer: Some(vec![0]),
      ..default()
    };

    let message = Keepsake {
      transfers: vec![Transfer {
        id: RELIC_ID,
        amount: 0,
        // split minted amount among all outputs
        output: 1 + u32::try_from(outputs).unwrap(),
      }],
      ..default()
    };

    // reveal and immediately burn an inception inscription per requested mint
    let inputs: Vec<(usize, usize, usize, Witness)> = (0..n)
      .map(|i| {
        (
          block_count + 1,
          1,
          i as usize,
          inception_inscription.to_witness(),
        )
      })
      .collect();
    let txid = self.core.broadcast_tx(TransactionTemplate {
      inputs: &inputs,
      outputs,
      // assign dust value to all outputs
      output_values: vec![dust_value.to_sat(); outputs].as_slice(),
      // put a value of 1 sat at index 0 to burn all the inscriptions
      op_return_index: Some(0),
      op_return_value: Some(1),
      op_return: Some(message.encipher()),
      // assign all unused sats as fee
      fee: (block_reward - dust_value * outputs as u64 - Amount::ONE_SAT).to_sat(),
      ..default()
    });

    self.mine_blocks(1);

    let mut entry = Self::base_token_entry();
    entry.state.mints += u128::from(n);

    (txid, entry)
  }

  /// Returns a list of Outpoints that total at least the given amount of relics.
  pub(crate) fn relic_outpoints(&self, relics: Vec<(RelicId, u128)>) -> Vec<OutPoint> {
    // find UTXOs to satisfy input requirements
    let mut outpoints = Vec::new();
    let mut allocated: HashMap<RelicId, u128> = HashMap::new();
    // collect used UTXOs from the mempool
    let mempool_outpoints: Vec<OutPoint> = self
      .core
      .mempool()
      .iter()
      .flat_map(|tx| &tx.input)
      .map(|input| input.previous_output)
      .collect();
    // iterate once over all available UTXOs with relic balances
    for (outpoint, balances) in self.index.get_relic_balances().unwrap().iter() {
      // skip outpoints already used within the mempool
      if mempool_outpoints.contains(outpoint) {
        continue;
      }
      // check every input requirement
      for (required_id, required_amount) in &relics {
        // check if requirement is not met yet
        if allocated.get(required_id).cloned().unwrap_or_default() < *required_amount {
          // and the current outpoint can contribute towards that requirement
          if balances
            .iter()
            .any(|(id, amount)| id == required_id && *amount > 0)
          {
            // add outpoint to inputs
            outpoints.push(*outpoint);
            // update allocated balances
            for (id, amount) in balances {
              *allocated.entry(*id).or_default() += amount;
            }
            // skip to the next outpoint
            break;
          }
        }
      }
    }
    // check if all required balances are met
    assert!(
      relics
        .iter()
        .all(|(id, amount)| allocated.get(id).cloned().unwrap_or_default() >= *amount),
      "unable to satisfy required Relic inputs, want {:?}, have {:?}",
      relics,
      allocated,
    );
    outpoints
  }

  pub(crate) fn relic_tx(
    &self,
    input_outpoints: &[OutPoint],
    outputs: usize,
    message: Keepsake,
  ) -> Txid {
    self.core.broadcast_tx(TransactionTemplate {
      input_outpoints,
      outputs,
      op_return: Some(message.encipher()),
      ..default()
    })
  }

  pub(crate) fn enshrine(&self, relic: SpacedRelic, enshrining: Enshrining) -> (Txid, RelicId) {
    let block_count = usize::try_from(self.index.block_count().unwrap()).unwrap();

    self.mine_blocks(1);

    self.core.broadcast_tx(TransactionTemplate {
      inputs: &[(block_count, 0, 0, Witness::new())],
      p2tr: true,
      ..default()
    });

    self.mine_blocks(Keepsake::COMMIT_CONFIRMATIONS.into());

    let keepsake = Keepsake {
      sealing: true,
      enshrining: Some(enshrining),
      // put any Relics into output number 2 to separate it from the Inscription
      pointer: Some(1),
      ..default()
    };

    let mut metadata = Vec::new();
    ciborium::into_writer(&relic.to_metadata(), &mut metadata).expect("Serialization failed");
    let relic_inscription = Inscription {
      metadata: Some(metadata),
      ..default()
    };

    let txid = self.core.broadcast_tx(TransactionTemplate {
      inputs: &[
        // reveal Inscription with SpacedRelic
        (block_count + 2, 0, 0, relic_inscription.to_witness()),
      ],
      input_outpoints: &self.relic_outpoints(vec![(RELIC_ID, relic.relic.sealing_fee())]),
      op_return: Some(keepsake.encipher()),
      outputs: 2,
      ..default()
    });

    self.mine_blocks(1);

    (
      txid,
      RelicId {
        block: u64::try_from(block_count + usize::from(Keepsake::COMMIT_CONFIRMATIONS) + 1)
          .unwrap(),
        tx: 1,
      },
    )
  }

  pub(crate) fn etch(&self, runestone: Runestone, outputs: usize) -> (Txid, RuneId) {
    let block_count = usize::try_from(self.index.block_count().unwrap()).unwrap();

    self.mine_blocks(1);

    self.core.broadcast_tx(TransactionTemplate {
      inputs: &[(block_count, 0, 0, Witness::new())],
      p2tr: true,
      ..default()
    });

    self.mine_blocks(Runestone::COMMIT_CONFIRMATIONS.into());

    let mut witness = Witness::new();

    if let Some(etching) = runestone.etching {
      let tapscript = script::Builder::new()
        .push_slice::<&PushBytes>(
          etching
            .rune
            .unwrap()
            .commitment()
            .as_slice()
            .try_into()
            .unwrap(),
        )
        .into_script();

      witness.push(tapscript);
    } else {
      witness.push(ScriptBuf::new());
    }

    witness.push([]);

    let txid = self.core.broadcast_tx(TransactionTemplate {
      inputs: &[(block_count + 1, 1, 0, witness)],
      op_return: Some(runestone.encipher()),
      outputs,
      ..default()
    });

    self.mine_blocks(1);

    (
      txid,
      RuneId {
        block: u64::try_from(block_count + usize::from(Runestone::COMMIT_CONFIRMATIONS) + 1)
          .unwrap(),
        tx: 1,
      },
    )
  }

  #[track_caller]
  pub(crate) fn assert_events(
    &self,
    receiver: &mut tokio::sync::mpsc::Receiver<Event>,
    expected: Vec<Event>,
  ) {
    let actual: Vec<Event> = expected.iter().flat_map(|_| receiver.try_recv()).collect();
    pretty_assert_eq!(expected, actual);
    assert!(receiver.is_empty(), "unexpected events: {:#?}", {
      let mut unexpected = Vec::new();
      while !receiver.is_empty() {
        unexpected.push(receiver.blocking_recv().unwrap());
      }
      unexpected
    });
    let mut tx_events: HashMap<Txid, Vec<Event>> = HashMap::new();
    let mut relic_events: HashMap<Relic, Vec<Event>> = HashMap::new();
    for event in expected {
      tx_events.entry(event.txid).or_default().push(event.clone());
      if event.is_relic_history() {
        let relic = self
          .index
          .get_relic_by_id(event.relic_id().unwrap())
          .unwrap()
          .unwrap();
        relic_events.entry(relic).or_default().push(event);
      }
    }
    for (txid, events) in tx_events {
      pretty_assert_eq!(events, self.index.events_for_tx(txid).unwrap());
    }
    for (relic_id, events) in relic_events {
      let mut actual = self.index.events_for_relic(relic_id, 100, 0).unwrap();
      if let Some(events) = actual.as_mut() {
        // the API returns items from new to old, hence reverse the order here
        events.reverse();
      }
      pretty_assert_eq!(Some(events), actual);
    }
  }
}
