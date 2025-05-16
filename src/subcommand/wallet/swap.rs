use super::*;

#[derive(Debug, Parser)]
pub(crate) struct Swap {
  #[clap(long, help = "Use <FEE_RATE> sats/vbyte for mint transaction.")]
  fee_rate: FeeRate,
  #[clap(
    long,
    help = "Swap Input Relic, defaults to RELIC. May contain `.` or `•` as spacers."
  )]
  input: Option<SpacedRelic>,
  #[clap(
    long,
    help = "Swap Output Relic, defaults to RELIC. May contain `.` or `•` as spacers."
  )]
  output: Option<SpacedRelic>,
  #[clap(long, help = "Swap Input amount <INPUT>.")]
  input_amount: Option<Decimal>,
  #[clap(long, help = "Swap Output amount <OUTPUT>.")]
  output_amount: Option<Decimal>,
  #[clap(long, help = "Use exact-input instead of exact-output swap.")]
  exact_input: bool,
  #[clap(
    long,
    help = "Include <AMOUNT> postage with mint output. [default: 546sat]"
  )]
  postage: Option<Amount>,
  #[clap(long, help = "Send output relics to <DESTINATION>.")]
  destination: Option<Address<NetworkUnchecked>>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Output {
  pub address: Address<NetworkUnchecked>,
  pub swap: Txid,
  pub input: SpacedRelic,
  pub input_amount: Pile,
  pub output: SpacedRelic,
  pub output_amount: Pile,
}

impl Swap {
  pub(crate) fn run(self, wallet: Wallet) -> SubcommandResult {
    ensure!(
      wallet.has_relic_index(),
      "`ord wallet swap` requires index created with `--index-relics` flag",
    );

    let base_token = SpacedRelic::from_str(RELIC_NAME)?;
    let input = self.input.unwrap_or(base_token);
    let output = self.output.unwrap_or(base_token);
    let (input_id, input_entry, _) = wallet
      .get_relic(input.relic)?
      .ok_or_else(|| anyhow!("input relic not found: {}", input))?;
    let (output_id, output_entry, _) = wallet
      .get_relic(output.relic)?
      .ok_or_else(|| anyhow!("output relic not found: {}", output))?;

    ensure!(
      input_id != output_id,
      "input and output Relic must not be the same"
    );

    let input_amount = self
      .input_amount
      .map(|amount| amount.to_integer(Enshrining::DIVISIBILITY))
      .transpose()?;
    let output_amount = self
      .output_amount
      .map(|amount| amount.to_integer(Enshrining::DIVISIBILITY))
      .transpose()?;

    let required_input_balance = input_amount.expect("swap CLI requires an input-amount for now");

    let (inputs, input_balances) =
      wallet.get_required_relic_outputs(vec![(input, required_input_balance)])?;
    let input_balance = input_balances.get(&input).cloned().unwrap_or_default();

    // if we do an exact-input swap and our inputs match the required input balance exactly the TX can be simplified
    let omit_relic_change_output =
      self.exact_input && input_balance == required_input_balance && input_balances.len() == 1;

    let postage = self.postage.unwrap_or(MIN_POSTAGE);

    let destination = match self.destination {
      Some(destination) => destination.require_network(wallet.chain().network())?,
      None => wallet.get_change_address()?,
    };

    ensure!(
      destination.script_pubkey().minimal_non_dust() < postage,
      "postage below dust limit of {}sat",
      destination.script_pubkey().minimal_non_dust().to_sat()
    );

    let transfers = if omit_relic_change_output {
      vec![]
    } else {
      // if our inputs contain other Relics too or there might be some input Relics left over after the swap
      // all of these will go to the first non-OP_RETURN output, i.e. output index 1
      // the swap output Relics will be assigned to output index 2
      vec![Transfer {
        id: output_id,
        amount: 0,
        output: 2,
      }]
    };

    let keepsake = Keepsake {
      swap: Some(relics_protocol::Swap {
        input: (input_id != RELIC_ID).then_some(input_id),
        output: (output_id != RELIC_ID).then_some(output_id),
        input_amount,
        output_amount,
        is_exact_input: self.exact_input,
      }),
      transfers,
      ..default()
    };

    let unfunded_transaction = Transaction {
      version: Version(2),
      lock_time: LockTime::ZERO,
      input: inputs
        .into_iter()
        .map(|previous_output| TxIn {
          previous_output,
          script_sig: ScriptBuf::new(),
          sequence: Sequence::MAX,
          witness: Witness::new(),
        })
        .collect(),
      output: if omit_relic_change_output {
        vec![
          TxOut {
            script_pubkey: keepsake.encipher(),
            value: Amount::ZERO,
          },
          TxOut {
            script_pubkey: destination.script_pubkey(),
            value: postage,
          },
        ]
      } else {
        vec![
          TxOut {
            script_pubkey: keepsake.encipher(),
            value: Amount::ZERO,
          },
          TxOut {
            script_pubkey: wallet.get_change_address()?.script_pubkey(),
            value: postage,
          },
          TxOut {
            script_pubkey: destination.script_pubkey(),
            value: postage,
          },
        ]
      },
    };

    wallet.lock_non_cardinal_outputs()?;

    let unsigned_transaction = fund_raw_transaction(
      wallet.bitcoin_client(),
      self.fee_rate,
      &unfunded_transaction,
    )?;

    let signed_transaction = wallet
      .bitcoin_client()
      .sign_raw_transaction_with_wallet(&unsigned_transaction, None, None)?
      .hex;

    let signed_transaction = consensus::encode::deserialize(&signed_transaction)?;

    assert_eq!(
      Keepsake::decipher(&signed_transaction),
      Some(RelicArtifact::Keepsake(keepsake)),
    );

    let transaction = wallet
      .bitcoin_client()
      .send_raw_transaction(&signed_transaction)?;

    Ok(Some(Box::new(Output {
      address: Address::<NetworkUnchecked>::from_str(&destination.to_string())?,
      swap: transaction,
      input,
      input_amount: Pile {
        amount: input_amount.unwrap_or_default(),
        divisibility: Enshrining::DIVISIBILITY,
        symbol: input_entry.symbol,
      },
      output,
      output_amount: Pile {
        amount: output_amount.unwrap_or_default(),
        divisibility: Enshrining::DIVISIBILITY,
        symbol: output_entry.symbol,
      },
    })))
  }
}
