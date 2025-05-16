use super::*;

#[derive(Boilerplate, Debug, PartialEq, Serialize, Deserialize)]
pub struct RelicEventsHtml {
  pub spaced_relic: SpacedRelic,
  pub events: Vec<Event>,
}

impl PageContent for RelicEventsHtml {
  fn title(&self) -> String {
    format!("Relic Events {}", self.spaced_relic)
  }
}
