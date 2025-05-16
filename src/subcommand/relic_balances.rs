use super::*;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Output {
  pub relics: BTreeMap<SpacedRelic, BTreeMap<OutPoint, Pile>>,
}

pub(crate) fn run(settings: Settings) -> SubcommandResult {
  let index = Index::open(&settings)?;

  ensure!(
    index.has_relic_index(),
    "`ord relic-balances` requires index created with `--index-relics` flag",
  );

  index.update()?;

  Ok(Some(Box::new(Output {
    relics: index.get_relic_balance_map()?,
  })))
}
