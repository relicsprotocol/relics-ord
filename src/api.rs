use {
  super::*,
  serde_hex::{SerHex, Strict},
};

pub use crate::{
  subcommand::decode::RawOutput as Decode,
  templates::{
    BlocksHtml as Blocks, RelicEventsHtml as RelicEvents, RelicHtml, RuneHtml as Rune,
    RunesHtml as Runes, StatusHtml as Status, TransactionHtml as Transaction,
  },
};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Block {
  pub best_height: u32,
  pub hash: BlockHash,
  pub height: u32,
  pub inscriptions: Vec<InscriptionId>,
  pub relics: Vec<SpacedRelic>,
  pub runes: Vec<SpacedRune>,
  pub target: BlockHash,
  pub transactions: Vec<bitcoin::blockdata::transaction::Transaction>,
}

impl Block {
  pub(crate) fn new(
    block: bitcoin::Block,
    height: Height,
    best_height: Height,
    inscriptions: Vec<InscriptionId>,
    relics: Vec<SpacedRelic>,
    runes: Vec<SpacedRune>,
  ) -> Self {
    Self {
      hash: block.header.block_hash(),
      target: target_as_block_hash(block.header.target()),
      height: height.0,
      best_height: best_height.0,
      inscriptions,
      relics,
      runes,
      transactions: block.txdata,
    }
  }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct BlockInfo {
  pub average_fee: u64,
  pub average_fee_rate: u64,
  pub bits: u32,
  #[serde(with = "SerHex::<Strict>")]
  pub chainwork: [u8; 32],
  pub confirmations: i32,
  pub difficulty: f64,
  pub hash: BlockHash,
  pub feerate_percentiles: [u64; 5],
  pub height: u32,
  pub max_fee: u64,
  pub max_fee_rate: u64,
  pub max_tx_size: u32,
  pub median_fee: u64,
  pub median_time: Option<u64>,
  pub merkle_root: TxMerkleNode,
  pub min_fee: u64,
  pub min_fee_rate: u64,
  pub next_block: Option<BlockHash>,
  pub nonce: u32,
  pub previous_block: Option<BlockHash>,
  pub subsidy: u64,
  pub target: BlockHash,
  pub timestamp: u64,
  pub total_fee: u64,
  pub total_size: usize,
  pub total_weight: usize,
  pub transaction_count: u64,
  pub version: u32,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Children {
  pub ids: Vec<InscriptionId>,
  pub more: bool,
  pub page: usize,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct ChildInscriptions {
  pub children: Vec<ChildInscriptionRecursive>,
  pub more: bool,
  pub page: usize,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct Inscription {
  pub address: Option<String>,
  pub charms: Vec<Charm>,
  pub child_count: u64,
  pub children: Vec<InscriptionId>,
  pub content_length: Option<usize>,
  pub content_type: Option<String>,
  pub effective_content_type: Option<String>,
  pub fee: u64,
  pub height: u32,
  pub id: InscriptionId,
  pub next: Option<InscriptionId>,
  pub number: i32,
  pub parents: Vec<InscriptionId>,
  pub previous: Option<InscriptionId>,
  // if this Inscription has sealed a Relic ticker
  pub relic_sealed: Option<SpacedRelic>,
  // if the sealed Relic ticker has already been enshrined
  pub relic_enshrined: bool,
  pub rune: Option<SpacedRune>,
  pub sat: Option<ordinals::Sat>,
  pub satpoint: SatPoint,
  pub timestamp: i64,
  pub value: Option<u64>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct InscriptionRecursive {
  pub charms: Vec<Charm>,
  pub content_type: Option<String>,
  pub content_length: Option<usize>,
  pub delegate: Option<InscriptionId>,
  pub fee: u64,
  pub height: u32,
  pub id: InscriptionId,
  pub number: i32,
  pub output: OutPoint,
  pub sat: Option<ordinals::Sat>,
  pub satpoint: SatPoint,
  pub timestamp: i64,
  pub value: Option<u64>,
  pub address: Option<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct ChildInscriptionRecursive {
  pub charms: Vec<Charm>,
  pub fee: u64,
  pub height: u32,
  pub id: InscriptionId,
  pub number: i32,
  pub output: OutPoint,
  pub sat: Option<ordinals::Sat>,
  pub satpoint: SatPoint,
  pub timestamp: i64,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Inscriptions {
  pub ids: Vec<InscriptionId>,
  pub more: bool,
  pub page_index: u32,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct RelicInscription {
  pub id: InscriptionId,
  pub is_base_relic: bool,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct Output {
  pub address: Option<Address<NetworkUnchecked>>,
  pub indexed: bool,
  pub inscriptions: Vec<RelicInscription>,
  pub outpoint: OutPoint,
  pub relics: BTreeMap<SpacedRelic, Pile>,
  pub runes: BTreeMap<SpacedRune, Pile>,
  pub sat_ranges: Option<Vec<(u64, u64)>>,
  pub script_pubkey: ScriptBuf,
  pub spent: bool,
  pub transaction: Txid,
  pub value: u64,
}

impl Output {
  pub fn new(
    chain: Chain,
    inscriptions: Vec<RelicInscription>,
    outpoint: OutPoint,
    tx_out: TxOut,
    indexed: bool,
    relics: BTreeMap<SpacedRelic, Pile>,
    runes: BTreeMap<SpacedRune, Pile>,
    sat_ranges: Option<Vec<(u64, u64)>>,
    spent: bool,
  ) -> Self {
    Self {
      address: chain
        .address_from_script(&tx_out.script_pubkey)
        .ok()
        .map(|address| uncheck(&address)),
      indexed,
      inscriptions,
      outpoint,
      relics,
      runes,
      sat_ranges,
      script_pubkey: tx_out.script_pubkey,
      spent,
      transaction: outpoint.txid,
      value: tx_out.value.to_sat(),
    }
  }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Sat {
  pub block: u32,
  pub charms: Vec<Charm>,
  pub cycle: u32,
  pub decimal: String,
  pub degree: String,
  pub epoch: u32,
  pub inscriptions: Vec<InscriptionId>,
  pub name: String,
  pub number: u64,
  pub offset: u64,
  pub percentile: String,
  pub period: u32,
  pub rarity: Rarity,
  pub satpoint: Option<SatPoint>,
  pub timestamp: i64,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct SatInscription {
  pub id: Option<InscriptionId>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct SatInscriptions {
  pub ids: Vec<InscriptionId>,
  pub more: bool,
  pub page: u64,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct AddressInfo {
  pub outputs: Vec<OutPoint>,
  pub inscriptions: Vec<InscriptionId>,
  pub sat_balance: u64,
  pub runes_balances: Vec<(SpacedRune, Decimal, Option<char>)>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Relic {
  #[serde(flatten)]
  pub entry: RelicEntry,
  pub id: RelicId,
  pub mintable: bool,
  pub owner: Option<InscriptionId>,
  pub thumb: Option<InscriptionId>,
}
