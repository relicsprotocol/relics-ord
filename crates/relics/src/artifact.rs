use super::*;

#[allow(clippy::large_enum_variant)]
#[derive(Serialize, Eq, PartialEq, Deserialize, Debug)]
pub enum RelicArtifact {
  Cenotaph(RelicCenotaph),
  Keepsake(Keepsake),
}
