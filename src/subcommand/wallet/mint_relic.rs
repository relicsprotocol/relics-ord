use {super::*, relics_protocol::MultiMint};

#[derive(Debug, Parser)]
pub(crate) struct MintRelic {
  #[clap(long, help = "Use <FEE_RATE> sats/vbyte for transaction.")]
  fee_rate: FeeRate,
  #[clap(
    long,
    help = "Mint or unmint <RELIC>. May contain `.` or `•` as spacers."
  )]
  relic: SpacedRelic,
  #[clap(long, help = "Include <AMOUNT> postage with output. [default: 546sat]")]
  postage: Option<Amount>,
  #[clap(
    long,
    help = "Send minted relics to <DESTINATION>. Not applicable for unminting yet."
  )]
  destination: Option<Address<NetworkUnchecked>>,
  #[clap(long, help = "Unmint the specified <RELIC> instead of minting.")]
  unmint: bool,
  #[clap(
    long,
    default_value = "1",
    help = "Number of mint operations to perform in a single transaction."
  )]
  num_mints: u8,
  #[clap(
    long,
    default_value = "10",
    help = "Maximum percentage slippage allowed. [default: 10%]"
  )]
  slippage: u64,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Output {
  pub unmint: Option<bool>,
  pub txid: Txid,
  pub address: Option<Address<NetworkUnchecked>>, // Address receiving relics (mint) or base relics (unmint)
  pub pile: Pile,                                 // Pile of minted relics or received base relics
  pub relic: Option<SpacedRelic>,                 // The relic that was minted or unminted
  pub price: u128, // Total price paid (for mint) or received (for unmint)
}

impl MintRelic {
  pub(crate) fn run(self, wallet: Wallet) -> SubcommandResult {
    ensure!(
      wallet.has_relic_index(),
      "`ord wallet mint-relic` requires index created with `--index-relics` flag",
    );

    let base_token = SpacedRelic::from_str(RELIC_NAME)?;
    ensure!(self.relic != base_token, "use mint-base-relic instead",);

    if self.unmint {
      ensure!(
        self.destination.is_none(),
        "--destination is not supported for --unmint yet"
      );

      // Get relic details (ID, entry, price)
      let (self_relic_id, entry, _) = wallet
        .get_relic(self.relic.relic)?
        .ok_or_else(|| anyhow!("relic {} has not been enshrined", self.relic))?;

      let (_, base_relic_entry, _) = wallet
        .get_relic(base_token.relic)?
        .ok_or_else(|| anyhow!("base relic {} not found", base_token))?;

      let result = entry
        .unmintable(u128::MAX, self.num_mints, 0)
        .map_err(|err| anyhow!("relic not unmintable {}: {}", self.relic, err))?;

      // Extract the first element from the vector
      let (unmint_amount, price) = result.first().ok_or_else(|| {
        anyhow!(
          "multi_unmintable returned empty result for relic {}",
          self.relic
        )
      })?;
      let unmint_amount = *unmint_amount;
      let price = *price;

      // Find input UTXOs containing the relic
      let (inputs, input_relic_balances) =
        wallet.get_required_relic_outputs(vec![(self.relic, unmint_amount)])?;

      // Check input balance
      let input_balance = input_relic_balances.get(&self.relic).cloned().unwrap_or(0);

      ensure!(
        input_balance >= unmint_amount,
        "insufficient balance of relic {}: have {}, need {}",
        self.relic,
        Pile {
          amount: input_balance,
          divisibility: Enshrining::DIVISIBILITY,
          symbol: entry.symbol
        },
        Pile {
          amount: unmint_amount,
          divisibility: Enshrining::DIVISIBILITY,
          symbol: entry.symbol
        },
      );

      // Determine if relic change output is needed
      let self_relic_change_amount = input_balance.saturating_sub(unmint_amount);
      let needs_relic_change_output =
        self_relic_change_amount > 0 || input_relic_balances.len() > 1;

      // Use change address as destination for base relics for now
      let base_relic_destination = wallet.get_change_address()?;

      // Determine postage for the base relic output
      let postage = self.postage.unwrap_or(MIN_POSTAGE);
      ensure!(
        base_relic_destination.script_pubkey().minimal_non_dust() < postage,
        "postage below dust limit of {}sat",
        base_relic_destination
          .script_pubkey()
          .minimal_non_dust()
          .to_sat()
      );

      // Construct Keepsake
      let keepsake = if needs_relic_change_output {
        let mut transfers = Vec::new();
        if self_relic_change_amount > 0 {
          // Change for the unminted relic goes to output 2
          transfers.push(Transfer {
            id: self_relic_id,
            amount: self_relic_change_amount,
            output: 2,
          });
        }
        // Base relics implicitly go to output 1
        Keepsake {
          transfers,
          mint: Some(MultiMint {
            base_limit: 0, // Not applicable for unmint
            count: 1,      // Unminting one unit of the relic
            is_unmint: true,
            relic: self_relic_id,
          }),
          ..default()
        }
      } else {
        // Base relics implicitly go to output 1
        Keepsake {
          mint: Some(MultiMint {
            base_limit: 0,
            count: 1,
            is_unmint: true,
            relic: self_relic_id,
          }),
          ..default()
        }
      };

      // Create script_pubkey
      let script_pubkey = keepsake.encipher();

      // Select the input with the highest self-relic balance
      let selected_input = if input_relic_balances.contains_key(&self.relic) {
        // Sort inputs by their self relic balance
        let mut input_balances = Vec::new();
        for input in &inputs {
          if let Ok(balances) = wallet.get_relics_balances_in_output(input) {
            if let Some(pile) = balances.get(&self.relic) {
              input_balances.push((input, pile.amount));
            }
          }
        }

        // Sort by balance (descending) and take the input with highest balance
        input_balances.sort_by(|a, b| b.1.cmp(&a.1));
        input_balances
          .first()
          .map(|(input, _)| **input)
          .unwrap_or(*inputs.first().unwrap())
      } else {
        *inputs.first().unwrap()
      };

      let sat_point = SatPoint {
        outpoint: selected_input,
        offset: 0,
      };

      // Determine change addresses: [0] for sats/base relics, [1] for unminted relic change
      let change = [base_relic_destination.clone(), wallet.get_change_address()?];

      let relic_outputs = wallet.get_relic_outputs()?;
      let runic_outputs = wallet.get_runic_outputs()?;

      // Use TransactionBuilder to build the transaction
      let target = Target::ExactPostage(postage); // Postage for the base relic output (output 1)

      wallet.lock_non_cardinal_outputs()?;

      let unsigned_transaction = TransactionBuilder::new(
        sat_point,
        wallet.inscriptions().clone(),
        wallet.utxos().clone(),
        wallet.locked_utxos().clone().into_keys().collect(),
        relic_outputs,
        runic_outputs,
        script_pubkey,
        change,
        self.fee_rate,
        target,
        wallet.chain().network(),
      )
      .build_transaction()?;

      let signed_transaction = wallet
        .bitcoin_client()
        .sign_raw_transaction_with_wallet(&unsigned_transaction, None, None)?
        .hex;

      let signed_transaction = consensus::encode::deserialize(&signed_transaction)?;

      // Sanity check keepsake
      assert_eq!(
        Keepsake::decipher(&signed_transaction),
        Some(RelicArtifact::Keepsake(keepsake)),
        "Keepsake decipher mismatch"
      );

      let transaction =
        wallet.send_raw_transaction(&signed_transaction, Some(Amount::from_sat(546)))?;

      // The base relics are implicitly assigned to the first non-OP_RETURN output (index 1)
      // let base_relic_output_index = 1;
      // let base_relic_outpoint = OutPoint { txid, vout: base_relic_output_index };

      Ok(Some(Box::new(Output {
        unmint: Some(true),
        txid: transaction,
        address: Some(Address::<NetworkUnchecked>::from_str(
          &base_relic_destination.to_string(),
        )?),
        pile: Pile {
          amount: price, // Amount of base relics created equals the price
          divisibility: Enshrining::DIVISIBILITY, // Base relic divisibility
          symbol: base_relic_entry.symbol, // Base relic symbol
        },
        relic: Some(self.relic),
        price,
      })))
    } else {
      let (id, entry, _) = wallet
        .get_relic(self.relic.relic)?
        .ok_or_else(|| anyhow!("relic {} has not been enshrined", self.relic))?;

      let total_minted = entry.state.mints;
      let total_price = entry
        .mint_terms
        .unwrap()
        .compute_total_price(total_minted, self.num_mints)
        .unwrap();

      // Apply slippage to the price
      let slippage_multiplier = 1.0 + (self.slippage as f64 / 100.0);
      let adjusted_price = total_price as f64 * slippage_multiplier;
      if !adjusted_price.is_finite()
        || adjusted_price.is_sign_negative()
        || adjusted_price > u64::MAX as f64
      {
        bail!("invalid adjusted_price: {}", adjusted_price);
      }
      #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
      let total_price_with_slippage = u128::from(adjusted_price.round() as u64);

      let (inputs, input_relic_balances) =
        wallet.get_required_relic_outputs(vec![(base_token, total_price_with_slippage)])?;

      let base_balance = input_relic_balances
        .get(&base_token)
        .cloned()
        .unwrap_or_default();

      // Apply slippage to the mintable check
      let result = entry
        .mintable(base_balance, self.num_mints, total_price_with_slippage)
        .map_err(|err| anyhow!("relic not mintable {}: {}", self.relic, err))?;

      // Extract the first element from the vector
      let (amount, price) = result.first().ok_or_else(|| {
        anyhow!(
          "multi_mintable returned empty result for relic {}",
          self.relic
        )
      })?;
      let amount = *amount;
      let price = *price;

      ensure!(
        base_balance >= price,
        "insufficient balance of base relic {}: have {}, need {}",
        base_token,
        Pile {
          amount: base_balance,
          divisibility: Enshrining::DIVISIBILITY,
          symbol: wallet.get_relic(base_token.relic)?.unwrap().1.symbol
        },
        Pile {
          amount: price,
          divisibility: Enshrining::DIVISIBILITY,
          symbol: wallet.get_relic(base_token.relic)?.unwrap().1.symbol
        },
      );

      let needs_relic_change_output = base_balance > price || input_relic_balances.len() > 1;

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

      let keepsake = if needs_relic_change_output {
        // base relic change -> output 1
        // newly minted relics -> output 2
        Keepsake {
          mint: Some(MultiMint {
            base_limit: total_price_with_slippage, // Specify the base price with slippage
            count: self.num_mints,                 // Minting specified number of the relic
            is_unmint: false,
            relic: id,
          }),
          transfers: vec![Transfer {
            id,        // The ID of the newly minted relic
            amount,    // The amount of the new relic being minted
            output: 2, // Send newly minted relic to output 2
          }],
          ..default()
        }
      } else {
        // no base relic change, minted relic goes to output 1 implicitly? No, must specify with MultiMint.
        // Let's be explicit: base relics consumed, new relic to output 1.
        Keepsake {
          mint: Some(MultiMint {
            base_limit: total_price_with_slippage,
            count: self.num_mints,
            is_unmint: false,
            relic: id,
          }),
          // Transfer not strictly needed if only one output (besides OP_RETURN),
          // but helps clarity/consistency. Let's keep it explicit for output 1.
          transfers: vec![Transfer {
            id,
            amount,
            output: 1, // Send newly minted relic to output 1
          }],
          ..default()
        }
      };

      // Create script pubkey for the OP_RETURN output containing the keepsake
      let script_pubkey = keepsake.encipher();

      // Create the transaction using all inputs
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
        output: if needs_relic_change_output {
          vec![
            TxOut {
              script_pubkey,
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
        } else {
          vec![
            TxOut {
              script_pubkey,
              value: Amount::ZERO,
            },
            TxOut {
              script_pubkey: destination.script_pubkey(),
              value: postage,
            },
          ]
        },
      };

      let unsigned_transaction = fund_raw_transaction(
        wallet.bitcoin_client(),
        self.fee_rate,
        &unfunded_transaction,
      )?;

      let unsigned_transaction = consensus::encode::deserialize(&unsigned_transaction)?;

      // Sanity check keepsake
      assert_eq!(
        Keepsake::decipher(&unsigned_transaction),
        Some(RelicArtifact::Keepsake(keepsake)),
        "Keepsake decipher mismatch"
      );

      let signed_transaction = wallet
        .bitcoin_client()
        .sign_raw_transaction_with_wallet(&unsigned_transaction, None, None)?
        .hex;

      // The signed transaction is a hex string, so no need to deserialize it again
      let transaction =
        wallet.send_raw_transaction(&signed_transaction, Some(Amount::from_sat(546)))?;

      Ok(Some(Box::new(Output {
        unmint: Some(false),
        txid: transaction,
        address: Some(Address::<NetworkUnchecked>::from_str(
          &destination.to_string(),
        )?),
        pile: Pile {
          amount,
          divisibility: Enshrining::DIVISIBILITY,
          symbol: entry.symbol,
        },
        relic: Some(self.relic),
        price: total_price_with_slippage,
      })))
    }
  }
}
