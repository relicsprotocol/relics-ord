use {
  super::*,
  ord::{relics::RELIC_NAME, subcommand::wallet::swap},
};

fn setup() -> (mockcore::Handle, TestServer) {
  let core = mockcore::builder().network(Network::Regtest).build();
  let ord = TestServer::spawn_with_server_args(
    &core,
    &["--index-relics", "--regtest", "--integration-test"],
    &[],
  );
  create_wallet(&core, &ord);
  (core, ord)
}

// Helper function to inscribe and mint the base token
fn mint_base(core: &mockcore::Handle, ord: &TestServer) -> Option<InscriptionId> {
  let (first_inscription, _) = inscribe(core, ord);
  let (second_inscription, _) = inscribe_with_parent(core, ord, first_inscription);
  let inscription_id = Some(second_inscription);

  let base = SpacedRelic::from_str(RELIC_NAME).unwrap();
  relic_mint(core, ord, base, 1, inscription_id, None);

  inscription_id
}

#[test]
fn mint_base_token_works() {
  let (core, ord) = setup();
  mint_base(&core, &ord);

  let balance = relic_balance(&core, &ord);

  // Check that base relic exists and has the expected balance
  assert!(balance.relics.is_some());
  let relics = balance.relics.unwrap();
  assert!(relics.contains_key(&SpacedRelic::from_str(RELIC_NAME).unwrap()));
  let base_balance = relics
    .get(&SpacedRelic::from_str(RELIC_NAME).unwrap())
    .unwrap();
  assert_eq!(base_balance.value, 654205);
  assert_eq!(base_balance.scale, 2);
}

#[test]
fn sealing_relic_works() {
  let (core, ord) = setup();
  mint_base(&core, &ord);
  let relic = SpacedRelic::from_str("BASIC•TEST•RELIC").unwrap();
  seal(&core, &ord, relic);
}

#[test]
fn enshrine_relic_works() {
  let (core, ord) = setup();
  mint_base(&core, &ord);
  let relic = SpacedRelic::from_str("BASIC•TEST•RELIC").unwrap();
  seal(&core, &ord, relic);
  relic_enshrine(&core, &ord, relic, 0, 0);
}

#[test]
fn enshrine_curved_relic_works() {
  let (core, ord) = setup();
  mint_base(&core, &ord);
  let relic = SpacedRelic::from_str("BASIC•TEST•RELIC").unwrap();
  seal(&core, &ord, relic);
  relic_enshrine_curved(&core, &ord, relic, 0, 0, 4_200_000);
}

#[test]
fn mint_relic_works() {
  let (core, ord) = setup();
  mint_base(&core, &ord);

  // create a new relic
  let relic = SpacedRelic::from_str("BASIC•TEST•RELIC").unwrap();
  seal(&core, &ord, relic);
  relic_enshrine(&core, &ord, relic, 0, 0);

  relic_mint(&core, &ord, relic, 1, None, None);
  core.mine_blocks(1);

  let balance = relic_balance(&core, &ord);

  // Check that the relic exists in the balance
  assert!(balance.relics.is_some());
  let relics = balance.relics.unwrap();
  assert!(relics.contains_key(&relic));
  let relic_balance = relics.get(&relic).unwrap();
  assert_eq!(relic_balance.scale, 0);
  assert_eq!(relic_balance.value, 20);
}

#[test]
fn mint_curved_relic_works() {
  let (core, ord) = setup();
  mint_base(&core, &ord);

  // create a new relic
  let base = SpacedRelic::from_str("MBTC").unwrap();
  let relic = SpacedRelic::from_str("BASIC•TEST•RELIC").unwrap();
  seal(&core, &ord, relic);
  relic_enshrine_curved(&core, &ord, relic, 0, 0, 4_200_000);

  relic_mint(&core, &ord, relic, 1, None, None);
  relic_mint(&core, &ord, relic, 1, None, None);
  relic_mint(&core, &ord, relic, 1, None, None);
  core.mine_blocks(1);

  let balance = relic_balance(&core, &ord);

  // Check that the relic exists in the balance
  assert!(balance.relics.is_some());
  let relics = balance.relics.unwrap();
  assert!(relics.contains_key(&relic));
  let relic_balance = relics.get(&relic).unwrap();
  assert_eq!(relic_balance.scale, 0);
  assert_eq!(relic_balance.value, 3000);

  // Check base balance is reduced by 1 (sealing) + 0.01 (mint)
  assert!(relics.contains_key(&base));
  let base_balance = relics.get(&base).unwrap();
  assert_eq!(base_balance.value, 65402973487);
  assert_eq!(base_balance.scale, 7);
}

#[test]
fn mint_free_relic_works() {
  let (core, ord) = setup();
  mint_base(&core, &ord);

  // create a new relic
  let base = SpacedRelic::from_str("MBTC").unwrap();
  let relic = SpacedRelic::from_str("BASIC•FREE•RELIC").unwrap();
  seal(&core, &ord, relic);
  relic_enshrine_free(&core, &ord, relic);

  relic_mint(&core, &ord, relic, 1, None, None);
  relic_mint(&core, &ord, relic, 1, None, None);
  relic_mint(&core, &ord, relic, 1, None, None);
  core.mine_blocks(1);

  let balance = relic_balance(&core, &ord);

  // Check that the relic exists in the balance
  assert!(balance.relics.is_some());
  let relics = balance.relics.unwrap();
  assert!(relics.contains_key(&relic));
  let mut free_relic_balance = relics.get(&relic).unwrap();
  assert_eq!(free_relic_balance.scale, 0);
  assert_eq!(free_relic_balance.value, 60);

  // Check base balance is reduced by 101 (sealing + subsidy)
  assert!(relics.contains_key(&base));
  let base_balance = relics.get(&base).unwrap();
  assert_eq!(base_balance.value, 644105);
  assert_eq!(base_balance.scale, 2);

  // Check that swapping works (selling free relics)
  let command = format!(
    r#"
        --chain regtest
        --index-relics
        wallet swap
        --fee-rate 1
        --input {}
        --input-amount 10
        --output {}
        --exact-input
    "#,
    relic, base
  );

  let output = CommandBuilder::new(command)
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<swap::Output>();

  pretty_assert_eq!(output.output, base);
  pretty_assert_eq!(output.input, relic);
  pretty_assert_eq!(output.input_amount.amount, 1_000_000_000);

  let balance_after_swap = relic_balance(&core, &ord);
  let relics = balance_after_swap.relics.unwrap();
  assert!(relics.contains_key(&relic));
  free_relic_balance = relics.get(&relic).unwrap();
  assert_eq!(free_relic_balance.scale, 0);
  assert_eq!(free_relic_balance.value, 50);

  // output after swapping with 0.3%
  let base_balance = relics.get(&base).unwrap();
  assert_eq!(base_balance.value, 64420371287);
  assert_eq!(base_balance.scale, 7);
}

#[test]
fn mint_relic_with_zero_slippage_works() {
  let (core, ord) = setup();
  mint_base(&core, &ord);

  // create a new relic
  let relic = SpacedRelic::from_str("BASIC•TEST•RELIC").unwrap();
  seal(&core, &ord, relic);
  relic_enshrine_curved(&core, &ord, relic, 0, 0, 4_200_000);

  relic_mint(&core, &ord, relic, 1, None, Some(0));

  let balance = relic_balance(&core, &ord);

  // Check that the relic exists in the balance
  assert!(balance.relics.is_some());
  let relics = balance.relics.unwrap();
  assert!(relics.contains_key(&relic));
  let relic_balance = relics.get(&relic).unwrap();
  assert_eq!(relic_balance.scale, 0);
  assert_eq!(relic_balance.value, 1000);
}

#[test]
fn multi_mint_curved_relic_works() {
  let (core, ord) = setup();
  mint_base(&core, &ord);

  // create a new relic
  let base = SpacedRelic::from_str("MBTC").unwrap();
  let relic = SpacedRelic::from_str("BASIC•TEST•RELIC").unwrap();
  seal(&core, &ord, relic);
  relic_enshrine_curved(&core, &ord, relic, 0, 0, 4_200_000);

  relic_mint(&core, &ord, relic, 3, None, None);

  let balance = relic_balance(&core, &ord);

  // Check that the relic exists in the balance
  assert!(balance.relics.is_some());
  let relics = balance.relics.unwrap();
  assert!(relics.contains_key(&relic));
  let relic_balance = relics.get(&relic).unwrap();
  assert_eq!(relic_balance.scale, 0);
  assert_eq!(relic_balance.value, 3000);

  // Check base balance is reduced by 1 (sealing) + 0.01 (mint)
  assert!(relics.contains_key(&base));
  let base_balance = relics.get(&base).unwrap();
  assert_eq!(base_balance.value, 65402973487);
  assert_eq!(base_balance.scale, 7);
}

#[test]
#[should_panic(
  expected = "relic not mintable BASIC•FREE•RELIC: maximum mints per transaction exceeded"
)]
fn multi_mint_free_relic_breaks() {
  let (core, ord) = setup();
  mint_base(&core, &ord);

  // create a new relic
  let relic = SpacedRelic::from_str("BASIC•FREE•RELIC").unwrap();
  seal(&core, &ord, relic);
  relic_enshrine_free(&core, &ord, relic);

  let command = format!(
    r#"
        --chain regtest
        --index-relics
        wallet mint-relic
        --fee-rate 1
        --relic {}
        --num-mints 3
    "#,
    relic
  );

  CommandBuilder::new(command)
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<ord::subcommand::wallet::mint_relic::Output>();
}

#[test]
fn launch_curved_relic_swap_price_comparison() {
  fn compute_swap_spent(enshrine_amt: u64) -> u128 {
    let (core, ord) = setup();
    for _ in 0..4 {
      mint_base(&core, &ord);
    }
    let base = SpacedRelic::from_str("MBTC").unwrap();
    let relic = SpacedRelic::from_str("BASIC•TEST•RELIC").unwrap();
    seal(&core, &ord, relic);
    relic_enshrine_curved(&core, &ord, relic, 0, 0, u128::from(enshrine_amt));

    // drive price upward through repeated mints
    for _ in 0..167 {
      core.mine_blocks(1);
      CommandBuilder::new(format!(
        "--chain regtest --index-relics wallet mint-relic --fee-rate 1 \
                 --relic {} --num-mints 100",
        relic
      ))
      .core(&core)
      .ord(&ord)
      .run_and_deserialize_output::<ord::subcommand::wallet::mint_relic::Output>();
      core.mine_blocks(1);
      ord.sync_server();
    }
    core.mine_blocks(1);
    CommandBuilder::new(format!(
      "--chain regtest --index-relics wallet mint-relic --fee-rate 1 \
             --relic {} --num-mints 99",
      relic
    ))
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<ord::subcommand::wallet::mint_relic::Output>();
    core.mine_blocks(1);
    ord.sync_server();

    core.mine_blocks(1);
    CommandBuilder::new(format!(
      "--chain regtest --index-relics wallet mint-relic --fee-rate 1 \
             --relic {} --num-mints 1",
      relic
    ))
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<ord::subcommand::wallet::mint_relic::Output>();
    core.mine_blocks(1);
    ord.sync_server();

    // capture base-token balances before and after swap
    let before = *relic_balance(&core, &ord)
      .relics
      .unwrap()
      .get(&base)
      .unwrap();
    CommandBuilder::new(format!(
      "--chain regtest --index-relics wallet swap --fee-rate 1 \
             --input {} --input-amount 1000 --output {} --output-amount 1000",
      base, relic
    ))
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<swap::Output>();
    let after = *relic_balance(&core, &ord)
      .relics
      .unwrap()
      .get(&base)
      .unwrap();

    // normalize to common scale and compute spent
    let max_scale = before.scale.max(after.scale);
    let norm = |v: u128, s: u8| {
      if s < max_scale {
        v * 10u128.pow((max_scale - s) as u32)
      } else {
        v
      }
    };
    norm(before.value, before.scale) - norm(after.value, after.scale)
  }

  let alt_price = compute_swap_spent(4_200_000 / 2);
  let std_price = compute_swap_spent(4_200_000);
  let high_price = compute_swap_spent(4_200_000 * 2);

  println!("alt_price  = {}", alt_price);
  println!("std_price  = {}", std_price);
  println!("high_price = {}", high_price);

  // expect high seed to yield lowest swap price, and low seed the highest
  assert!(
    high_price < std_price && std_price < alt_price,
    "swap prices order wrong: high={} < standard={} < alt={}",
    high_price,
    std_price,
    alt_price
  );
}

#[test]
fn unmint_relic_works() {
  let (core, ord) = setup();
  mint_base(&core, &ord);

  // create a new relic
  let base = SpacedRelic::from_str("MBTC").unwrap();
  let relic = SpacedRelic::from_str("BASIC•TEST•RELIC").unwrap();
  seal(&core, &ord, relic);
  relic_enshrine(&core, &ord, relic, 0, 100);

  relic_mint(&core, &ord, relic, 1, None, None);

  let balance = relic_balance(&core, &ord);

  // Check that the relic exists in the balance
  assert!(balance.relics.is_some());
  let relics = balance.relics.unwrap();
  assert!(relics.contains_key(&relic));
  let relic_bal = relics.get(&relic).unwrap();
  assert_eq!(relic_bal.scale, 0);
  assert_eq!(relic_bal.value, 20);

  // Check base balance is reduced by 1 (sealing) + 0.01 (mint)
  assert!(relics.contains_key(&base));
  let base_balance = relics.get(&base).unwrap();
  assert_eq!(base_balance.scale, 2);
  assert_eq!(base_balance.value, 654104);

  relic_unmint(&core, &ord, relic);

  let new_balance = relic_balance(&core, &ord);
  // Check that the relic exists in the balance
  assert!(new_balance.relics.is_some());
  let relics = new_balance.relics.unwrap();
  assert!(!relics.contains_key(&relic));

  // Check base balance is increased again by 0.01 (unmint)
  assert!(relics.contains_key(&base));
  let new_base_balance = relics.get(&base).unwrap();
  assert_eq!(new_base_balance.scale, 2);
  assert_eq!(new_base_balance.value, 654105);
}

#[test]
fn swap_relic_works() {
  let (core, ord) = setup();
  mint_base(&core, &ord);

  let base = SpacedRelic::from_str(RELIC_NAME).unwrap();
  let relic = SpacedRelic::from_str("BASIC•TEST•RELIC").unwrap();
  seal(&core, &ord, relic);
  relic_enshrine(&core, &ord, relic, 0, 0);

  relic_mint(&core, &ord, relic, 1, None, None);
  relic_mint(&core, &ord, relic, 1, None, None);
  relic_mint(&core, &ord, relic, 1, None, None);

  let command = format!(
    r#"
        --chain regtest
        --index-relics
        wallet swap
        --fee-rate 1
        --input {}
        --input-amount 5
        --exact-input
    "#,
    relic
  );

  let output = CommandBuilder::new(command)
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<swap::Output>();

  pretty_assert_eq!(output.input, relic);
  pretty_assert_eq!(output.output, base);

  core.mine_blocks(1);

  let balances = CommandBuilder::new("--regtest --index-relics --index-addresses relic-balances")
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<ord::subcommand::relic_balances::Output>();

  let relic_balances: BTreeMap<SpacedRelic, u128> = balances
    .relics
    .iter()
    .map(|(relic, outpoints)| {
      (
        *relic,
        outpoints.iter().fold(0u128, |acc, v| acc + v.1.amount),
      )
    })
    .collect();

  // initial balance = 6542,05
  // sealing fee = 1
  // minting price = 0.03
  // selling 5 BASIC•TEST•RELIC for 0,00014925
  pretty_assert_eq!(
    relic_balances,
    vec![(base, 654102014925), (relic, 5500000000)]
      .into_iter()
      .collect()
  );
}

#[test]
fn swap_relic_works_with_fee() {
  let (core, ord) = setup();
  mint_base(&core, &ord);

  let base = SpacedRelic::from_str(RELIC_NAME).unwrap();
  let relic = SpacedRelic::from_str("BASIC•TEST•RELIC").unwrap();
  seal(&core, &ord, relic);
  relic_enshrine(&core, &ord, relic, 100, 0); // 1% fee

  relic_mint(&core, &ord, relic, 1, None, None);
  relic_mint(&core, &ord, relic, 1, None, None);
  relic_mint(&core, &ord, relic, 1, None, None);

  let command = format!(
    r#"
        --chain regtest
        --index-relics
        wallet swap
        --fee-rate 1
        --input {}
        --input-amount 5
        --exact-input
    "#,
    relic
  );

  println!("swap cmd: {command}");

  let output = CommandBuilder::new(command)
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<swap::Output>();

  pretty_assert_eq!(output.input, relic);
  pretty_assert_eq!(output.output, base);

  core.mine_blocks(1);

  let balances = CommandBuilder::new("--regtest --index-relics --index-addresses relic-balances")
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<ord::subcommand::relic_balances::Output>();

  let relic_balances: BTreeMap<SpacedRelic, u128> = balances
    .relics
    .iter()
    .map(|(relic, outpoints)| {
      (
        *relic,
        outpoints.iter().fold(0u128, |acc, v| acc + v.1.amount),
      )
    })
    .collect();

  // initial balance = 6542,05
  // sealing fee = 1
  // minting price = 0.03
  // selling 5 BASIC•TEST•RELIC for 0,00014776 (1% fee subtracted)
  pretty_assert_eq!(
    relic_balances,
    vec![(base, 654102014775), (relic, 5500000000)]
      .into_iter()
      .collect()
  );
}

#[test]
fn swap_relic_works_with_high_fee() {
  let (core, ord) = setup();
  mint_base(&core, &ord);

  let base = SpacedRelic::from_str(RELIC_NAME).unwrap();
  let relic = SpacedRelic::from_str("BASIC•TEST•RELIC").unwrap();
  seal(&core, &ord, relic);
  relic_enshrine(&core, &ord, relic, 1000, 0); // 10% fee

  relic_mint(&core, &ord, relic, 1, None, None);
  relic_mint(&core, &ord, relic, 1, None, None);
  relic_mint(&core, &ord, relic, 1, None, None);

  let command = format!(
    r#"
        --chain regtest
        --index-relics
        wallet swap
        --fee-rate 1
        --input {}
        --input-amount 5
        --exact-input
    "#,
    relic
  );

  println!("swap cmd: {command}");

  let output = CommandBuilder::new(command)
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<swap::Output>();

  pretty_assert_eq!(output.input, relic);
  pretty_assert_eq!(output.output, base);

  core.mine_blocks(1);

  let balances = CommandBuilder::new("--regtest --index-relics --index-addresses relic-balances")
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<ord::subcommand::relic_balances::Output>();

  let relic_balances: BTreeMap<SpacedRelic, u128> = balances
    .relics
    .iter()
    .map(|(relic, outpoints)| {
      (
        *relic,
        outpoints.iter().fold(0u128, |acc, v| acc + v.1.amount),
      )
    })
    .collect();

  // initial balance = 6542,05
  // sealing fee = 1
  // minting price = 0.03
  // selling 5 BASIC•TEST•RELIC for 0,00013432 (10% fee subtracted)
  pretty_assert_eq!(
    relic_balances,
    vec![(base, 654102013432), (relic, 5500000000)]
      .into_iter()
      .collect()
  );
}

#[test]
fn swap_fee_is_capped() {
  let (core, ord) = setup();
  mint_base(&core, &ord);

  let base = SpacedRelic::from_str(RELIC_NAME).unwrap();
  let relic = SpacedRelic::from_str("BASIC•TEST•RELIC").unwrap();
  seal(&core, &ord, relic);
  relic_enshrine(&core, &ord, relic, 1100, 0); // 11% -> will be capped at 10%

  relic_mint(&core, &ord, relic, 1, None, None);
  relic_mint(&core, &ord, relic, 1, None, None);
  relic_mint(&core, &ord, relic, 1, None, None);

  let command = format!(
    r#"
        --chain regtest
        --index-relics
        wallet swap
        --fee-rate 1
        --input {}
        --input-amount 5
        --exact-input
    "#,
    relic
  );

  println!("swap cmd: {command}");

  let output = CommandBuilder::new(command)
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<swap::Output>();

  pretty_assert_eq!(output.input, relic);
  pretty_assert_eq!(output.output, base);

  core.mine_blocks(1);

  let balances = CommandBuilder::new("--regtest --index-relics --index-addresses relic-balances")
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<ord::subcommand::relic_balances::Output>();

  let relic_balances: BTreeMap<SpacedRelic, u128> = balances
    .relics
    .iter()
    .map(|(relic, outpoints)| {
      (
        *relic,
        outpoints.iter().fold(0u128, |acc, v| acc + v.1.amount),
      )
    })
    .collect();

  // initial balance = 6542,05
  // sealing fee = 1
  // minting price = 0.03
  // selling 5 BASIC•TEST•RELIC for 0,00013432 (10% fee subtracted)
  pretty_assert_eq!(
    relic_balances,
    vec![(base, 654102013432), (relic, 5500000000)]
      .into_iter()
      .collect()
  );
}
