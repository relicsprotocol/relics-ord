use super::*;

#[derive(Debug, Parser)]
pub(crate) struct Enshrine {
  #[clap(long, help = "Use <FEE_RATE> sats/vbyte for mint transaction.")]
  fee_rate: FeeRate,
  #[clap(long, help = "Amount of <RELIC> per mint.")]
  mint_amount: Decimal,
  #[clap(long, help = "Number of possible <RELIC> mints.")]
  mint_cap: u128,
  #[clap(
    long,
    help = "Price in RELIC per <RELIC>. Cannot be used with formula price parameters."
  )]
  mint_price: Option<Decimal>,
  #[clap(
    long,
    help = "Formula price parameter a. Price = a - b/(c+x) where x is the mint index."
  )]
  formula_a: Option<u128>,
  #[clap(
    long,
    help = "Formula price parameter b. Price = a - b/(c+x) where x is the mint index."
  )]
  formula_b: Option<u128>,
  #[clap(long, help = "Maximum transactions that can mint <RELIC>.")]
  tx_cap: Option<u8>,
  #[clap(
    long,
    help = "Include <AMOUNT> postage with enshrine output. [default: 546sat]"
  )]
  postage: Option<Amount>,
  #[clap(
    long,
    help = "Enshrine <RELIC>. May contain `.` or `•` as spacers. Must have been sealed."
  )]
  relic: SpacedRelic,
  #[clap(
    long,
    help = "Amount of <RELIC> to be added to the initial liquidity pool after mint out."
  )]
  seed: Decimal,
  #[clap(
    long,
    help = "Amount of MBTC to be sponsor as lp seed (only if price = 0)."
  )]
  subsidy: Option<Decimal>,
  #[clap(
    long,
    help = "Symbol for the enshrined relic. Must be a single char (unicode)."
  )]
  symbol: char,
  #[clap(
    long,
    help = "Liquidity pool fee in basis points (0-10000). [default: 0]"
  )]
  lp_fee: Option<u16>,
  #[clap(long, help = "Max unmints for the enshrined relic. [default: 0]")]
  max_unmints: Option<u32>,
  #[clap(long, help = "Simulate the command without sending the transaction.")]
  dry_run: bool,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Output {
  pub enshrining: Txid,
  pub relic: SpacedRelic,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub dry_run: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub keepsake: Option<Keepsake>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub signed_transaction: Option<String>,
}

impl Enshrine {
  pub(crate) fn run(self, wallet: Wallet) -> SubcommandResult {
    if self.dry_run {
      println!(
        "DRY RUN: Simulating enshrining of relic {} - no transaction will be sent",
        self.relic
      );
    }

    ensure!(
      wallet.has_relic_index(),
      "`ord wallet enshrine` requires index created with `--index-relics` flag",
    );

    // Check that either fixed price or all formula parameters are provided, but not both
    let has_fixed_price = self.mint_price.is_some();
    let has_formula = self.formula_a.is_some() || self.formula_b.is_some();

    ensure!(
      has_fixed_price || has_formula,
      "either mint_price or formula parameters (a, b) must be specified"
    );

    ensure!(
      !(has_fixed_price && has_formula),
      "cannot specify both mint_price and formula parameters"
    );

    if has_formula {
      ensure!(
        self.formula_a.is_some() && self.formula_b.is_some(),
        "all formula parameters (a, b) must be specified"
      );
    }

    let price_model = if has_fixed_price {
      PriceModel::Fixed(
        self
          .mint_price
          .unwrap()
          .to_integer(Enshrining::DIVISIBILITY)?,
      )
    } else {
      PriceModel::Formula {
        a: self.formula_a.unwrap(),
        b: self.formula_b.unwrap(),
      }
    };

    let sealing = wallet
      .inscription_info()
      .iter()
      .find(|(_, inscription)| inscription.relic_sealed == Some(self.relic))
      .map(|(_, inscription)| {
        (
          inscription.satpoint,
          inscription
            .address
            .as_ref()
            .expect("unsupported script on the sealing inscription")
            .clone(),
        )
      });

    let Some((sealing_satpoint, sealing_address)) = sealing else {
      panic!("sealing inscription not found for relic: {}", self.relic);
    };

    let postage = self.postage.unwrap_or(MIN_POSTAGE);
    let change_addresses = [wallet.get_change_address()?, wallet.get_change_address()?]; // For fund_raw_transaction if it needs them explicitly, or for our own use
    let destination_address =
      Address::from_str(&sealing_address)?.require_network(wallet.chain().network())?;

    ensure!(
      destination_address.script_pubkey().minimal_non_dust() < postage,
      "postage below dust limit of {}sat",
      destination_address
        .script_pubkey()
        .minimal_non_dust()
        .to_sat()
    );

    let mut inputs_for_tx = Vec::new();
    let mut outputs_for_tx_payload = Vec::new(); // Non-OP_RETURN outputs
    let mut keepsake_transfers = Vec::new();

    // Add sealing satpoint input
    inputs_for_tx.push(TxIn {
      previous_output: sealing_satpoint.outpoint,
      script_sig: ScriptBuf::new(),
      sequence: Sequence::MAX,
      witness: Witness::new(),
    });

    let base_token_spaced_relic = SpacedRelic::from_str(RELIC_NAME)?; // RELIC

    let has_subsidy = self.subsidy.is_some() && self.subsidy.unwrap().value > 0;

    // Order: Inscription Output, then Relic Change Output (if any)

    // 1. Add Sealing Inscription Output (will be absolute index 1)
    outputs_for_tx_payload.push(TxOut {
      script_pubkey: destination_address.script_pubkey(),
      value: postage,
    });

    // 2. Conditionally add Base RELIC change output (will be absolute index 2)
    if has_subsidy {
      let subsidy_decimal = self.subsidy.unwrap();
      let subsidy_amount_integer = subsidy_decimal.to_integer(Enshrining::DIVISIBILITY)?;

      if subsidy_amount_integer > 0 {
        let (relic_utxos_to_spend, input_relic_balances) = wallet
          .get_required_relic_outputs(vec![(base_token_spaced_relic, subsidy_amount_integer)])?;

        for utxo in relic_utxos_to_spend {
          inputs_for_tx.push(TxIn {
            previous_output: utxo,
            script_sig: ScriptBuf::new(),
            sequence: Sequence::MAX,
            witness: Witness::new(),
          });
        }

        let base_tokens_from_inputs = input_relic_balances
          .get(&base_token_spaced_relic)
          .cloned()
          .unwrap_or(0);
        let base_relic_change_amount =
          base_tokens_from_inputs.saturating_sub(subsidy_amount_integer);

        if base_relic_change_amount > 0 {
          outputs_for_tx_payload.push(TxOut {
            value: postage, // Base relic change output also gets postage
            script_pubkey: change_addresses[0].script_pubkey(), // Use first change address for relic change
          });
          keepsake_transfers.push(relics_protocol::Transfer {
            id: RELIC_ID, // ID for the base RELIC token
            amount: base_relic_change_amount,
            output: 2, // <<< Absolute output index: 0=OP_RETURN, 1=Inscription, 2=Relic Change
          });
        }
      }
    }

    // Sealing inscription output is already added above.
    // let sealing_inscription_output_abs_idx = 1; // For reference

    let keepsake = Keepsake {
      enshrining: Some(Enshrining {
        symbol: Some(self.symbol),
        boost_terms: None,
        fee: self.lp_fee.filter(|&fee| fee > 0),
        subsidy: self
          .subsidy
          .filter(|subsidy| subsidy.value > 0)
          .map(|subsidy| subsidy.to_integer(Enshrining::DIVISIBILITY))
          .transpose()?,
        mint_terms: Some(MintTerms {
          amount: Some(self.mint_amount.to_integer(Enshrining::DIVISIBILITY)?),
          block_cap: None,
          cap: Some(self.mint_cap),
          max_unmints: self.max_unmints.filter(|&unmints| unmints > 0),
          price: Some(price_model),
          seed: Some(self.seed.to_integer(Enshrining::DIVISIBILITY)?),
          tx_cap: self.tx_cap,
        }),
      }),
      transfers: keepsake_transfers, // Add relic transfers here
      ..default()
    };

    let op_return_script_pubkey = keepsake.encipher();

    ensure!(
      op_return_script_pubkey.len() <= 82,
      "keepsake greater than maximum OP_RETURN size: {} > 82",
      op_return_script_pubkey.len()
    );

    let mut final_outputs = Vec::new();
    final_outputs.push(TxOut {
      value: Amount::ZERO,
      script_pubkey: op_return_script_pubkey,
    });
    final_outputs.extend(outputs_for_tx_payload);

    let unfunded_transaction = Transaction {
      version: Version(2),
      lock_time: LockTime::ZERO,
      input: inputs_for_tx,
      output: final_outputs,
    };

    wallet.lock_non_cardinal_outputs()?;

    let bitcoin_client = wallet.bitcoin_client();
    let unsigned_transaction =
      fund_raw_transaction(bitcoin_client, self.fee_rate, &unfunded_transaction)?;

    let signed_transaction = wallet
      .bitcoin_client()
      .sign_raw_transaction_with_wallet(&unsigned_transaction, None, None)?
      .hex;

    let signed_transaction = consensus::encode::deserialize(&signed_transaction)?;

    // Clone the keepsake before using it in the assertion
    let keepsake_clone = keepsake.clone();

    assert_eq!(
      Keepsake::decipher(&signed_transaction),
      Some(RelicArtifact::Keepsake(keepsake_clone)),
    );

    if self.dry_run {
      // For dry run, include more details in the output
      Ok(Some(Box::new(Output {
        enshrining: Txid::all_zeros(),
        relic: self.relic,
        dry_run: Some(true),
        keepsake: Some(keepsake),
        signed_transaction: Some(hex::encode(consensus::encode::serialize(
          &signed_transaction,
        ))),
      })))
    } else {
      let transaction = wallet
        .bitcoin_client()
        .send_raw_transaction(&signed_transaction)?;

      Ok(Some(Box::new(Output {
        enshrining: transaction,
        relic: self.relic,
        dry_run: None,
        keepsake: None,
        signed_transaction: None,
      })))
    }
  }
}
