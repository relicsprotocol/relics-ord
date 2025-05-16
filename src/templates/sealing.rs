use super::*;

#[derive(Boilerplate, Debug, Serialize, Deserialize)]
pub struct SealingHtml {
  pub inscription: api::Inscription,
  pub enshrining_tx: Option<Txid>,
}

impl PageContent for SealingHtml {
  fn title(&self) -> String {
    "Ticker".to_string()
  }
}
