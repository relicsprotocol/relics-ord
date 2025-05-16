use {super::*, bincode::Options, redb::TypeName, std::cmp::Ordering};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EventInfo {
  InscriptionCreated {
    charms: u16,
    inscription_id: InscriptionId,
    location: Option<SatPoint>,
    parent_inscription_ids: Vec<InscriptionId>,
    sequence_number: u32,
  },
  InscriptionTransferred {
    inscription_id: InscriptionId,
    new_location: SatPoint,
    old_location: SatPoint,
    sequence_number: u32,
    is_base_relic: bool,
  },
  RuneBurned {
    rune_id: RuneId,
    amount: u128,
  },
  RuneEtched {
    rune_id: RuneId,
  },
  RuneMinted {
    rune_id: RuneId,
    amount: u128,
  },
  RuneTransferred {
    rune_id: RuneId,
    amount: u128,
    output: u32,
  },
  RelicSealed {
    spaced_relic: SpacedRelic,
    sequence_number: u32,
    inscription_id: InscriptionId,
  },
  RelicBurned {
    relic_id: RelicId,
    amount: u128,
  },
  RelicEnshrined {
    relic_id: RelicId,
    inscription_id: InscriptionId,
  },
  RelicMinted {
    relic_id: RelicId,
    amount: u128,
    multiplier: u32,
    is_unmint: bool,
  },
  RelicMultiMinted {
    relic_id: RelicId,
    amount: u128,
    num_mints: u8,
    base_limit: u128,
    is_unmint: bool,
  },
  RelicSpent {
    relic_id: RelicId,
    amount: u128,
    // utility field for apps, could also store output and calc address only on read
    address: Address<NetworkUnchecked>,
  },
  RelicReceived {
    relic_id: RelicId,
    amount: u128,
    // utility field for apps, could also store output and calc address only on read
    address: Address<NetworkUnchecked>,
  },
  RelicTransferred {
    relic_id: RelicId,
    amount: u128,
    output: u32,
  },
  RelicSwapped {
    relic_id: RelicId,
    base_amount: u128,
    quote_amount: u128,
    fee: u128,
    is_sell_order: bool,
    is_exact_input: bool,
  },
  RelicClaimed {
    amount: u128,
  },
  RelicError {
    operation: RelicOperation,
    error: RelicError,
  },
}

#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
pub enum RelicOperation {
  Seal,
  Enshrine,
  Mint,
  MultiMint,
  Unmint,
  MultiUnmint,
  Swap,
  Claim,
}

impl Display for Event {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    write!(f, "{:?}", self)
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
  pub block_height: u32,
  pub event_index: u32,
  pub txid: Txid,
  pub info: EventInfo,
}

impl Event {
  pub fn is_relic_history(&self) -> bool {
    matches!(
      self.info,
      EventInfo::RelicMinted { .. }
        | EventInfo::RelicBurned { .. }
        | EventInfo::RelicSpent { .. }
        | EventInfo::RelicReceived { .. }
        | EventInfo::RelicTransferred { .. }
        | EventInfo::RelicSwapped { .. }
    )
  }

  pub fn relic_id(&self) -> Option<RelicId> {
    match self.info {
      EventInfo::RelicEnshrined { relic_id, .. } => Some(relic_id),
      EventInfo::RelicMinted { relic_id, .. } => Some(relic_id),
      EventInfo::RelicBurned { relic_id, .. } => Some(relic_id),
      EventInfo::RelicSpent { relic_id, .. } => Some(relic_id),
      EventInfo::RelicReceived { relic_id, .. } => Some(relic_id),
      EventInfo::RelicTransferred { relic_id, .. } => Some(relic_id),
      EventInfo::RelicSwapped { relic_id, .. } => Some(relic_id),
      EventInfo::RelicClaimed { .. } => Some(RELIC_ID),
      _ => None,
    }
  }
}

impl redb::Value for Event {
  type SelfType<'a>
    = Self
  where
    Self: 'a;
  type AsBytes<'a>
    = Vec<u8>
  where
    Self: 'a;

  fn fixed_width() -> Option<usize> {
    None
  }

  fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
  where
    Self: 'a,
  {
    let options = bincode::DefaultOptions::new();
    options.deserialize(data).unwrap()
  }

  fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
  where
    Self: 'a,
    Self: 'b,
  {
    let options = bincode::DefaultOptions::new();
    options.serialize(value).unwrap()
  }

  fn type_name() -> TypeName {
    TypeName::new("Event")
  }
}

impl redb::Key for Event {
  fn compare(data1: &[u8], data2: &[u8]) -> Ordering {
    // Note: If performance becomes an issue here because of deserialization,
    // we should be able to sort the items based on their byte representation.
    // For that to work we need to disable varint encoding, switch to big-endian,
    // and then sort based on the first 8 bytes of each item.
    // data1.cmp(data2)
    let options = bincode::DefaultOptions::new();
    let item1: Event = options.deserialize(data1).unwrap();
    let item2: Event = options.deserialize(data2).unwrap();
    let key1 = (item1.block_height, item1.event_index);
    let key2 = (item2.block_height, item2.event_index);
    key1.cmp(&key2)
  }
}

pub struct EventEmitter<'a, 'tx> {
  pub block_height: u32,
  pub event_index: u32,
  pub event_sender: Option<&'a tokio::sync::mpsc::Sender<Event>>,
  pub relic_id_to_events: &'a mut MultimapTable<'tx, RelicIdValue, Event>,
  pub transaction_id_to_events: &'a mut MultimapTable<'tx, &'static TxidValue, Event>,
}

impl EventEmitter<'_, '_> {
  pub fn emit(&mut self, txid: Txid, info: EventInfo) -> Result {
    let event = Event {
      block_height: self.block_height,
      event_index: self.event_index,
      txid,
      info,
    };
    self.event_index += 1;
    if let Some(sender) = self.event_sender {
      sender.blocking_send(event.clone())?;
    }
    // store all events with the TX
    self
      .transaction_id_to_events
      .insert(&txid.store(), &event)?;
    // store some of the events with the relic
    if event.is_relic_history() {
      if let Some(relic_id) = event.relic_id() {
        self.relic_id_to_events.insert(relic_id.store(), &event)?;
      }
    }

    Ok(())
  }
}
