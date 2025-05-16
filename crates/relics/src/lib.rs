extern crate core;

use {
  bitcoin::{
    constants::MAX_SCRIPT_ELEMENT_SIZE, opcodes, script, script::Instruction, Network, ScriptBuf,
    Transaction,
  },
  ordinals::Rune,
  serde::{Deserialize, Serialize},
  serde_with::{DeserializeFromStr, SerializeDisplay},
  std::{
    collections::{HashMap, VecDeque},
    fmt::{self, Debug, Display, Formatter},
    str::FromStr,
  },
};

pub use {
  artifact::RelicArtifact,
  cenotaph::RelicCenotaph,
  enshrining::{BoostTerms, Enshrining, MintTerms, MultiMint, PriceModel},
  flaw::RelicFlaw,
  keepsake::Keepsake,
  ordinals::{varint, RuneId as RelicId},
  pool::*,
  relic::Relic,
  spaced_relic::SpacedRelic,
  swap::Swap,
  transfer::Transfer,
};

macro_rules! ensure {
  ($cond:expr, $err:expr) => {
    if !$cond {
      return Err($err);
    }
  };
}

pub const RELIC_ID: RelicId = RelicId { block: 1, tx: 0 };
pub const RELIC_NAME: &str = "MBTC";
pub const INCEPTION_PARENT_INSCRIPTION_ID: &str =
  "4e00929ef9849c20364d331e9b25d40b2f2f2ef8081d3cc769fd83c78d075f05i0";

#[cfg(test)]
fn default<T: Default>() -> T {
  Default::default()
}

mod artifact;
mod cenotaph;
mod enshrining;
mod flaw;
mod keepsake;
mod pool;
mod relic;
pub mod spaced_relic;
mod swap;
mod transfer;
