pub(super) enum Flag {
  // Commitment = 0,
  Sealing = 1,
  Enshrining = 2,
  MintTerms = 3,
  Swap = 4,
  SwapExactInput = 5,
  MultiMint = 6,
  BoostTerms = 7,
  #[allow(unused)]
  Cenotaph = 127,
}

impl Flag {
  pub(super) fn mask(self) -> u128 {
    1 << self as u128
  }

  pub(super) fn take(self, flags: &mut u128) -> bool {
    let mask = self.mask();
    let set = *flags & mask != 0;
    *flags &= !mask;
    set
  }

  pub(super) fn set(self, flags: &mut u128) {
    *flags |= self.mask()
  }
}

impl From<Flag> for u128 {
  fn from(flag: Flag) -> Self {
    flag.mask()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn mask() {
    assert_eq!(Flag::Sealing.mask(), 0b10);
    assert_eq!(Flag::Cenotaph.mask(), 1 << 127);
  }

  #[test]
  fn take() {
    let mut flags = 4;
    assert!(Flag::Enshrining.take(&mut flags));
    assert_eq!(flags, 0);

    let mut flags = 0;
    assert!(!Flag::Enshrining.take(&mut flags));
    assert_eq!(flags, 0);
  }

  #[test]
  fn set() {
    let mut flags = 0;
    Flag::Enshrining.set(&mut flags);
    assert_eq!(flags, 4);
  }
}
