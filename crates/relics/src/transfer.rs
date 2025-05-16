use super::*;

/// Edict
#[derive(Default, Serialize, Deserialize, Debug, PartialEq, Copy, Clone, Eq)]
pub struct Transfer {
  /// specifies Token to allocate or swap
  pub id: RelicId,
  /// target amount to allocate to output
  /// special case: 0 means "all remaining"
  pub amount: u128,
  /// output number to receive Relics
  /// special case: if output is equal to the number of tx outputs, amount Relics are allocated to each non-OP_RETURN output
  /// super special case: if output is equal to the number of tx outputs AND amount is 0: all remaining Relics are split among all non-OP_RETURN outputs
  /// invalid case: output greater than the number of tx outputs
  pub output: u32,
}

impl Transfer {
  pub fn from_integers(tx: &Transaction, id: RelicId, amount: u128, output: u128) -> Option<Self> {
    let Ok(output) = u32::try_from(output) else {
      return None;
    };

    // note that this allows `output == tx.output.len()`, which means to divide
    // amount between all non-OP_RETURN outputs
    if output > u32::try_from(tx.output.len()).unwrap() {
      return None;
    }

    Some(Self { id, amount, output })
  }
}
