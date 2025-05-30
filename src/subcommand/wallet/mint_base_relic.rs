use {super::*, relics_protocol::INCEPTION_PARENT_INSCRIPTION_ID};

#[derive(Debug, Parser)]
pub(crate) struct MintBaseRelic {
  #[clap(long, help = "Use <FEE_RATE> sats/vbyte for transaction.")]
  fee_rate: FeeRate,
  #[clap(long, help = "Include <AMOUNT> postage with output. [default: 546sat]")]
  postage: Option<Amount>,
  #[clap(long, help = "Send minted base relics to <DESTINATION>.")]
  destination: Option<Address<NetworkUnchecked>>,
  #[clap(long, help = "Burn <INSCRIPTION> to mint base relic.")]
  inscription: InscriptionId, // Changed from Option<InscriptionId> as it's required now
  #[clap(
    long,
    default_value = "1",
    help = "Number of base relics to mint from one inscription burn."
  )]
  num_mints: u8,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Output {
  pub txid: Txid,
  pub address: Option<Address<NetworkUnchecked>>,
  pub pile: Pile,
  pub relic: Option<SpacedRelic>, // Keep consistent output structure, maybe remove later?
  pub operation: String,          // "mint_base"
  pub price: u128,
}

impl MintBaseRelic {
  pub(crate) fn run(self, wallet: Wallet) -> SubcommandResult {
    ensure!(
      wallet.has_relic_index(),
      "`ord wallet mint-base-relic` requires index created with `--index-relics` flag",
    );

    let inscription_info = wallet
      .inscription_info()
      .get(&self.inscription)
      .ok_or_else(|| anyhow!("inscription {} not found", self.inscription))?
      .clone();

    let Some(value) = inscription_info.value else {
      bail!("Cannot burn unbound inscription");
    };
    let value = Amount::from_sat(value);

    let base_token = SpacedRelic::from_str(RELIC_NAME)?;

    // --- Mint Base Relic via Inscription Burn ---
    // Verify inscription exists and is owned by the wallet
    let mut inscription_found = false;
    let mut inscription_location = None;

    for (location, inscriptions) in wallet.inscriptions() {
      if inscriptions.contains(&self.inscription) {
        inscription_found = true;
        inscription_location = Some(location);
        break;
      }
    }
    ensure!(
      inscription_found,
      "inscription {} is not owned by this wallet",
      self.inscription
    );

    let first_parent = wallet.get_first_parent(self.inscription)?;

    // Verify inscription has inception parent
    if !wallet.integration_test() {
      ensure!(
        wallet.is_inception_parent(first_parent)?,
        "inscription {} must have inception parent {}, but has {}",
        self.inscription,
        INCEPTION_PARENT_INSCRIPTION_ID,
        first_parent
      );
    }

    let inscription_location = inscription_location.expect("Failed to get inscription location");
    let (base_relic_id, base_relic_entry, _) = wallet
      .get_relic(base_token.relic)?
      .ok_or_else(|| anyhow!("base relic {} not found", base_token))?;

    // For base token minting, create a keepsake with a transfer to split the minted amount
    let keepsake = Keepsake {
      transfers: vec![Transfer {
        id: base_relic_id,
        amount: u128::from(self.num_mints),
        output: 2, // Send the newly minted base relic to output 2
      }],
      // Note: No MultiMint needed for inscription burning? TBC - Assuming burning *creates* the base relic representation
      ..default()
    };

    // Create script pubkey for the OP_RETURN output containing the keepsake
    let script_pubkey = keepsake.encipher();
    let postage = self.postage.unwrap_or(MIN_POSTAGE);

    let destination = self
      .destination
      .clone()
      .map(|d| d.require_network(wallet.chain().network()))
      .transpose()?
      .unwrap_or_else(|| {
        wallet
          .get_change_address()
          .expect("Failed to get change address")
      });

    ensure!(
      destination.script_pubkey().minimal_non_dust() < postage,
      "postage below dust limit of {}sat",
      destination.script_pubkey().minimal_non_dust().to_sat()
    );

    // change[0] for sat change (output 1), change[1] for the new base relic (output 2)
    let change = [wallet.get_change_address()?, destination.clone()];

    let relic_outputs = wallet.get_relic_outputs()?;
    let runic_outputs = wallet.get_runic_outputs()?;

    // Create the transaction using TransactionBuilder
    let unsigned_transaction = TransactionBuilder::new(
      *inscription_location, // The input is the UTXO holding the inscription to burn
      wallet.inscriptions().clone(),
      wallet.utxos().clone(),
      wallet.locked_utxos().clone().into_keys().collect(),
      relic_outputs,
      runic_outputs,
      script_pubkey, // OP_RETURN
      change,        // change[0] -> output 1 (sat change), change[1] -> output 2 (new base relic)
      self.fee_rate,
      Target::ExactPostage(postage), // Postage goes to the base relic output (output 2)
      wallet.chain().network(),
    )
    .build_transaction()?;

    let base_size = unsigned_transaction.base_size();
    assert!(
      base_size >= 65,
      "transaction base size less than minimum standard tx nonwitness size: {base_size} < 65",
    );

    let signed_transaction = wallet
      .bitcoin_client()
      .sign_raw_transaction_with_wallet(&unsigned_transaction, None, None)?
      .hex;

    let signed_transaction = consensus::encode::deserialize(&signed_transaction)?;

    assert_eq!(
      Keepsake::decipher(&signed_transaction),
      Some(RelicArtifact::Keepsake(keepsake)),
    );

    let transaction = wallet.send_raw_transaction(&signed_transaction, Some(value))?;

    Ok(Some(Box::new(Output {
      address: Some(Address::<NetworkUnchecked>::from_str(
        &destination.to_string(),
      )?),
      txid: transaction,
      pile: Pile {
        amount: u128::from(self.num_mints),
        divisibility: Enshrining::DIVISIBILITY,
        symbol: base_relic_entry.symbol,
      },
      relic: Some(base_token), // Return the base token relic
      operation: "mint_base".to_string(),
      price: 0,
    })))
  }
}
