use super::*;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Output {
  pub cardinal: u64,
  pub ordinal: u64,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub relics: Option<BTreeMap<SpacedRelic, Decimal>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub relicy: Option<u64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub runes: Option<BTreeMap<SpacedRune, Decimal>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub runic: Option<u64>,
  pub total: u64,
}

pub(crate) fn run(wallet: Wallet) -> SubcommandResult {
  let unspent_outputs = wallet.utxos();

  let inscription_outputs = wallet
    .inscriptions()
    .keys()
    .map(|satpoint| satpoint.outpoint)
    .collect::<BTreeSet<OutPoint>>();

  let mut cardinal = 0;
  let mut ordinal = 0;
  let mut relics = BTreeMap::new();
  let mut relicy = 0;
  let mut runes = BTreeMap::new();
  let mut runic = 0;

  for (output, txout) in unspent_outputs {
    let relic_balances = wallet.get_relics_balances_in_output(output)?;
    let rune_balances = wallet.get_runes_balances_in_output(output)?;

    let is_ordinal = inscription_outputs.contains(output);
    let is_relicy = !relic_balances.is_empty();
    let is_runic = !rune_balances.is_empty();

    if is_ordinal {
      ordinal += txout.value.to_sat();
    }

    if is_relicy {
      for (spaced_relic, pile) in relic_balances {
        relics
          .entry(spaced_relic)
          .and_modify(|decimal: &mut Decimal| {
            assert_eq!(decimal.scale, pile.divisibility);
            decimal.value += pile.amount;
          })
          .or_insert(Decimal {
            value: pile.amount,
            scale: pile.divisibility,
          });
      }
      relicy += txout.value.to_sat();
    }

    if is_runic {
      for (spaced_rune, pile) in rune_balances {
        runes
          .entry(spaced_rune)
          .and_modify(|decimal: &mut Decimal| {
            assert_eq!(decimal.scale, pile.divisibility);
            decimal.value += pile.amount;
          })
          .or_insert(Decimal {
            value: pile.amount,
            scale: pile.divisibility,
          });
      }
      runic += txout.value.to_sat();
    }

    match (is_ordinal, is_relicy, is_runic) {
      (false, false, false) => cardinal += txout.value.to_sat(),
      (true, true, true) => {
        eprintln!("warning: output {output} contains inscriptions, relics and runes")
      }
      (true, true, false) => {
        eprintln!("warning: output {output} contains both inscriptions and relics")
      }
      (true, false, true) => {
        eprintln!("warning: output {output} contains both inscriptions and runes")
      }
      (false, true, true) => {
        eprintln!("warning: output {output} contains both relics and runes")
      }
      _ => {}
    }
  }

  Ok(Some(Box::new(Output {
    cardinal,
    ordinal,
    relics: wallet.has_relic_index().then_some(relics),
    relicy: wallet.has_relic_index().then_some(relicy),
    runes: wallet.has_rune_index().then_some(runes),
    runic: wallet.has_rune_index().then_some(runic),
    total: cardinal + ordinal + relicy + runic,
  })))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn runes_and_runic_fields_are_not_present_if_none() {
    assert_eq!(
      serde_json::to_string(&Output {
        cardinal: 0,
        ordinal: 0,
        relics: None,
        relicy: None,
        runes: None,
        runic: None,
        total: 0
      })
      .unwrap(),
      r#"{"cardinal":0,"ordinal":0,"total":0}"#
    );
  }
}
