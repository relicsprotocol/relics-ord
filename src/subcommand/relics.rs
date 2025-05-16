use super::*;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Output {
  pub relics: BTreeMap<Relic, RelicInfo>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct RelicInfo {
  pub block: u64,
  pub divisibility: u8,
  pub enshrining: Txid,
  pub id: RelicId,
  pub number: u64,
  pub state: RelicState,
  pub relic: SpacedRelic,
  pub max_supply: u128,
  pub circulating_supply: u128,
  pub symbol: Option<char>,
  pub mint_terms: Option<MintTerms>,
  pub pool: Option<Pool>,
  pub timestamp: DateTime<Utc>,
  pub tx: u32,
}

pub(crate) fn run(settings: Settings) -> SubcommandResult {
  let index = Index::open(&settings)?;

  ensure!(
    index.has_relic_index(),
    "`ord relics` requires index created with `--index-relics` flag",
  );

  index.update()?;

  Ok(Some(Box::new(Output {
    relics: index
      .relics()?
      .into_iter()
      .map(
        |(
          id,
          entry @ RelicEntry {
            block,
            enshrining,
            number,
            spaced_relic,
            symbol,
            mint_terms,
            state,
            pool,
            timestamp,
            ..
          },
        )| {
          (
            spaced_relic.relic,
            RelicInfo {
              block,
              divisibility: Enshrining::DIVISIBILITY,
              enshrining,
              id,
              number,
              state,
              relic: spaced_relic,
              max_supply: entry.max_supply(),
              circulating_supply: entry.circulating_supply(),
              symbol,
              mint_terms,
              pool,
              timestamp: crate::timestamp(timestamp),
              tx: id.tx,
            },
          )
        },
      )
      .collect::<BTreeMap<Relic, RelicInfo>>(),
  })))
}
