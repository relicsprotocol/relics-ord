use super::*;

#[derive(Debug, Parser)]
pub(crate) struct Claim {
  #[clap(long, help = "Use <FEE_RATE> sats/vbyte for mint transaction.")]
  fee_rate: FeeRate,
  #[clap(
    long,
    help = "Include <AMOUNT> postage with mint output. [default: 546sat]"
  )]
  postage: Option<Amount>,
  #[clap(long, help = "Send minted relics to <DESTINATION>.")]
  destination: Option<Address<NetworkUnchecked>>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Output {
  pub address: Address<NetworkUnchecked>,
  pub claim: Txid,
}

impl Claim {
  pub(crate) fn run(self, wallet: Wallet) -> SubcommandResult {
    ensure!(
      wallet.has_relic_index(),
      "`ord wallet claim` requires index created with `--index-relics` flag",
    );

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

    let keepsake = Keepsake {
      claim: Some(1),
      ..default()
    };

    let unfunded_transaction = Transaction {
      version: Version(2),
      lock_time: LockTime::ZERO,
      input: vec![],
      output: vec![
        TxOut {
          script_pubkey: keepsake.encipher(),
          value: Amount::ZERO,
        },
        TxOut {
          script_pubkey: destination.script_pubkey(),
          value: postage,
        },
      ],
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
      claim: transaction,
    })))
  }
}
