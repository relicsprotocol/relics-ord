use super::*;

#[derive(Boilerplate, Debug, PartialEq, Serialize, Deserialize)]
pub struct RelicsHtml {
  pub entries: Vec<(RelicId, Option<InscriptionId>, RelicEntry)>,
  pub more: bool,
  pub prev: Option<usize>,
  pub next: Option<usize>,
}

impl PageContent for RelicsHtml {
  fn title(&self) -> String {
    "Relics".to_string()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn display() {
    assert_eq!(
      RelicsHtml {
        entries: vec![(
          RelicId { block: 0, tx: 0 },
          Some(InscriptionId {
            txid: txid(1),
            index: 0,
          }),
          RelicEntry {
            spaced_relic: SpacedRelic {
              relic: Relic(26),
              spacers: 1
            },
            ..default()
          }
        )],
        more: false,
        prev: None,
        next: None,
      }
      .to_string(),
      "<h1>Relics</h1>
<ul>
  <li><a href=/relic/A•A>A•A</a></li>
</ul>
<div class=center>
    prev
      next
  </div>"
    );
  }

  #[test]
  fn with_prev_and_next() {
    assert_eq!(
      RelicsHtml {
        entries: vec![
          (
            RelicId { block: 0, tx: 0 },
            None,
            RelicEntry {
              spaced_relic: SpacedRelic {
                relic: Relic(0),
                spacers: 0
              },
              ..Default::default()
            }
          ),
          (
            RelicId { block: 0, tx: 1 },
            None,
            RelicEntry {
              spaced_relic: SpacedRelic {
                relic: Relic(2),
                spacers: 0
              },
              ..Default::default()
            }
          )
        ],
        prev: Some(1),
        next: Some(2),
        more: true,
      }
      .to_string(),
      "<h1>Relics</h1>
<ul>
  <li><a href=/relic/A>A</a></li>
  <li><a href=/relic/C>C</a></li>
</ul>
<div class=center>
    <a class=prev href=/relics/1>prev</a>
      <a class=next href=/relics/2>next</a>
  </div>"
    );
  }
}
