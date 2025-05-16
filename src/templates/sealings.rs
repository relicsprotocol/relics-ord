use super::*;

#[derive(Boilerplate, Debug, Serialize, Deserialize)]
pub struct SealingsHtml {
  pub entries: Vec<(api::Inscription, Option<Txid>)>,
  pub more: bool,
  pub prev: Option<usize>,
  pub next: Option<usize>,
}

impl PageContent for SealingsHtml {
  fn title(&self) -> String {
    "Tickers".to_string()
  }
}
