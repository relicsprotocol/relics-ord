use super::*;

#[derive(Serialize, Deserialize)]
pub struct Output {
  pub address: String,
  pub relic: SpacedRelic,
}

pub(crate) fn run(wallet: Wallet) -> SubcommandResult {
  ensure!(
    wallet.has_relic_index(),
    "`ord wallet sealed` requires index created with `--index-relics` flag",
  );

  let sealed_relics: Vec<Output> = wallet
    .inscription_info()
    .iter()
    .filter(|(_, inscription)| inscription.relic_sealed.is_some())
    .map(|(_, inscription)| Output {
      address: inscription.address.clone().unwrap(),
      relic: inscription.relic_sealed.unwrap(),
    })
    .collect();

  Ok(Some(Box::new(sealed_relics)))
}
