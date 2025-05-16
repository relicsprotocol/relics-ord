use ciborium::Value;
use ordinals::{spaced_rune, SpacedRune};

use super::*;

#[derive(
  Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Default, Hash, DeserializeFromStr, SerializeDisplay,
)]
pub struct SpacedRelic {
  pub relic: Relic,
  pub spacers: u32,
}

impl SpacedRelic {
  pub const METADATA_KEY: &'static str = "RELIC";

  pub fn new(relic: Relic, spacers: u32) -> Self {
    Self { relic, spacers }
  }

  pub fn from_metadata(metadata: Value) -> Option<Self> {
    for (key, value) in metadata.as_map()? {
      if key.as_text() != Some(Self::METADATA_KEY) {
        continue;
      }
      return SpacedRelic::from_str(value.as_text()?).ok();
    }
    None
  }

  pub fn to_metadata(&self) -> Value {
    Value::Map(vec![(
      Value::Text(Self::METADATA_KEY.into()),
      Value::Text(self.to_string()),
    )])
  }
  pub fn to_metadata_yaml(&self) -> serde_yaml::Value {
    let mut mapping = serde_yaml::Mapping::new();
    mapping.insert(
      serde_yaml::Value::String(Self::METADATA_KEY.into()),
      serde_yaml::Value::String(self.to_string()),
    );
    serde_yaml::Value::Mapping(mapping)
  }
}

impl From<SpacedRune> for SpacedRelic {
  fn from(value: SpacedRune) -> Self {
    SpacedRelic {
      relic: Relic(value.rune.0),
      spacers: value.spacers,
    }
  }
}

impl From<SpacedRelic> for SpacedRune {
  fn from(value: SpacedRelic) -> Self {
    SpacedRune {
      rune: Rune(value.relic.0),
      spacers: value.spacers,
    }
  }
}

impl Debug for SpacedRelic {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    write!(f, "{}", self)
  }
}

impl FromStr for SpacedRelic {
  type Err = spaced_rune::Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    SpacedRune::from_str(s).map(SpacedRelic::from)
  }
}

impl Display for SpacedRelic {
  fn fmt(&self, f: &mut Formatter) -> fmt::Result {
    let rune = SpacedRune::from(*self);
    write!(f, "{}", rune)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn display() {
    assert_eq!("A.B".parse::<SpacedRelic>().unwrap().to_string(), "A•B");
    assert_eq!("A.B.C".parse::<SpacedRelic>().unwrap().to_string(), "A•B•C");
    assert_eq!(
      SpacedRelic {
        relic: Relic(0),
        spacers: 1
      }
      .to_string(),
      "A"
    );
  }

  #[test]
  fn from_str() {
    #[track_caller]
    fn case(s: &str, relic: &str, spacers: u32) {
      assert_eq!(
        s.parse::<SpacedRelic>().unwrap(),
        SpacedRelic {
          relic: relic.parse().unwrap(),
          spacers
        },
      );
    }

    assert_eq!(
      ".A".parse::<SpacedRelic>().unwrap_err(),
      spaced_rune::Error::LeadingSpacer,
    );

    assert_eq!(
      "A..B".parse::<SpacedRelic>().unwrap_err(),
      spaced_rune::Error::DoubleSpacer,
    );

    assert_eq!(
      "A.".parse::<SpacedRelic>().unwrap_err(),
      spaced_rune::Error::TrailingSpacer,
    );

    assert_eq!(
      "Ax".parse::<SpacedRelic>().unwrap_err(),
      spaced_rune::Error::Character('x')
    );

    case("A.B", "AB", 0b1);
    case("A.B.C", "ABC", 0b11);
    case("A•B", "AB", 0b1);
    case("A•B•C", "ABC", 0b11);
    case("A•BC", "ABC", 0b1);
  }

  #[test]
  fn serde() {
    let spaced_rune = SpacedRelic {
      relic: Relic(26),
      spacers: 1,
    };
    let json = "\"A•A\"";
    assert_eq!(serde_json::to_string(&spaced_rune).unwrap(), json);
    assert_eq!(
      serde_json::from_str::<SpacedRelic>(json).unwrap(),
      spaced_rune
    );
  }
}
