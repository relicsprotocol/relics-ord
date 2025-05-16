use super::*;

#[derive(Serialize, Eq, PartialEq, Deserialize, Debug, Default)]
pub struct RelicCenotaph {
  pub flaw: Option<RelicFlaw>,
}
