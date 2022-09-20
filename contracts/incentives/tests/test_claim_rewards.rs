use cosmwasm_std::testing::{mock_env, mock_info};
use cosmwasm_std::{
    attr, coins, Addr, BankMsg, CosmosMsg, Decimal, OverflowError, OverflowOperation, StdError,
    SubMsg, Timestamp, Uint128,
};

use mars_outpost::incentives::msg::ExecuteMsg;
use mars_outpost::incentives::AssetIncentive;
use mars_outpost::red_bank::{Market, UserCollateralResponse};
use mars_testing::MockEnvParams;

use mars_incentives::contract::{execute, query_user_unclaimed_rewards};
use mars_incentives::helpers::{asset_incentive_compute_index, user_compute_accrued_rewards};
use mars_incentives::state::{ASSET_INCENTIVES, USER_ASSET_INDICES, USER_UNCLAIMED_REWARDS};

use crate::helpers::setup_test;

mod helpers;

#[test]
fn test_execute_claim_rewards() {
    // SETUP
    let mut deps = setup_test();
    let user_addr = Addr::unchecked("user");

    let previous_unclaimed_rewards = Uint128::new(50_000);
    let asset_total_supply = Uint128::new(100_000);
    let asset_user_balance = Uint128::new(10_000);
    let zero_total_supply = Uint128::new(200_000);
    let zero_user_balance = Uint128::new(10_000);
    let no_user_total_supply = Uint128::new(100_000);
    let no_user_user_balance = Uint128::zero();
    let time_start = 500_000_u64;
    let time_contract_call = 600_000_u64;

    // addresses
    // denom of an asset with ongoing rewards
    let asset_denom = "asset";
    // denom of an asset with no pending rewards but with user index (so it had active incentives
    // at some point)
    let zero_denom = "zero";
    // denom of an asset where the user never had a balance during an active
    // incentive -> hence no associated index
    let no_user_denom = "no_user";

    deps.querier.set_redbank_market(Market {
        denom: asset_denom.to_string(),
        collateral_total_scaled: asset_total_supply,
        ..Default::default()
    });
    deps.querier.set_redbank_market(Market {
        denom: zero_denom.to_string(),
        collateral_total_scaled: zero_total_supply,
        ..Default::default()
    });
    deps.querier.set_redbank_market(Market {
        denom: no_user_denom.to_string(),
        collateral_total_scaled: no_user_total_supply,
        ..Default::default()
    });
    deps.querier.set_red_bank_user_collateral(
        &user_addr,
        UserCollateralResponse {
            denom: asset_denom.to_string(),
            amount_scaled: asset_user_balance,
            amount: Uint128::zero(), // doesn't matter for this test
            enabled: true,
        },
    );
    deps.querier.set_red_bank_user_collateral(
        &user_addr,
        UserCollateralResponse {
            denom: zero_denom.to_string(),
            amount_scaled: zero_user_balance,
            amount: Uint128::zero(), // doesn't matter for this test
            enabled: true,
        },
    );
    deps.querier.set_red_bank_user_collateral(
        &user_addr,
        UserCollateralResponse {
            denom: no_user_denom.to_string(),
            amount_scaled: no_user_user_balance,
            amount: Uint128::zero(), // doesn't matter for this test
            enabled: true,
        },
    );

    // incentives
    ASSET_INCENTIVES
        .save(
            deps.as_mut().storage,
            asset_denom,
            &AssetIncentive {
                emission_per_second: Uint128::new(100),
                index: Decimal::one(),
                last_updated: time_start,
            },
        )
        .unwrap();
    ASSET_INCENTIVES
        .save(
            deps.as_mut().storage,
            zero_denom,
            &AssetIncentive {
                emission_per_second: Uint128::zero(),
                index: Decimal::one(),
                last_updated: time_start,
            },
        )
        .unwrap();
    ASSET_INCENTIVES
        .save(
            deps.as_mut().storage,
            no_user_denom,
            &AssetIncentive {
                emission_per_second: Uint128::new(200),
                index: Decimal::one(),
                last_updated: time_start,
            },
        )
        .unwrap();

    // user indices
    USER_ASSET_INDICES
        .save(deps.as_mut().storage, (&user_addr, asset_denom), &Decimal::one())
        .unwrap();

    USER_ASSET_INDICES
        .save(deps.as_mut().storage, (&user_addr, zero_denom), &Decimal::from_ratio(1_u128, 2_u128))
        .unwrap();

    // unclaimed_rewards
    USER_UNCLAIMED_REWARDS
        .save(deps.as_mut().storage, &user_addr, &previous_unclaimed_rewards)
        .unwrap();

    let expected_asset_incentive_index = asset_incentive_compute_index(
        Decimal::one(),
        Uint128::new(100),
        asset_total_supply,
        time_start,
        time_contract_call,
    )
    .unwrap();

    let expected_asset_accrued_rewards = user_compute_accrued_rewards(
        asset_user_balance,
        Decimal::one(),
        expected_asset_incentive_index,
    )
    .unwrap();

    let expected_zero_accrued_rewards = user_compute_accrued_rewards(
        zero_user_balance,
        Decimal::from_ratio(1_u128, 2_u128),
        Decimal::one(),
    )
    .unwrap();

    let expected_accrued_rewards =
        previous_unclaimed_rewards + expected_asset_accrued_rewards + expected_zero_accrued_rewards;

    // MSG
    let info = mock_info("user", &[]);
    let env = mars_testing::mock_env(MockEnvParams {
        block_time: Timestamp::from_seconds(time_contract_call),
        ..Default::default()
    });
    let msg = ExecuteMsg::ClaimRewards {};

    // query a bit before gives less rewards
    let env_before = mars_testing::mock_env(MockEnvParams {
        block_time: Timestamp::from_seconds(time_contract_call - 10_000),
        ..Default::default()
    });
    let rewards_query_before =
        query_user_unclaimed_rewards(deps.as_ref(), env_before, String::from("user")).unwrap();
    assert!(rewards_query_before < expected_accrued_rewards);

    // query before execution gives expected rewards
    let rewards_query =
        query_user_unclaimed_rewards(deps.as_ref(), env.clone(), String::from("user")).unwrap();
    assert_eq!(rewards_query, expected_accrued_rewards);

    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    // query after execution gives 0 rewards
    let rewards_query_after =
        query_user_unclaimed_rewards(deps.as_ref(), env, String::from("user")).unwrap();
    assert_eq!(rewards_query_after, Uint128::zero());

    // ASSERT

    assert_eq!(
        res.messages,
        vec![SubMsg::new(CosmosMsg::Bank(BankMsg::Send {
            to_address: user_addr.to_string(),
            amount: coins(expected_accrued_rewards.u128(), "umars".to_string())
        }))]
    );

    assert_eq!(
        res.attributes,
        vec![
            attr("action", "outposts/incentives/claim_rewards"),
            attr("user", "user"),
            attr("mars_rewards", expected_accrued_rewards),
        ]
    );

    // asset and zero incentives get updated, no_user does not
    let asset_incentive = ASSET_INCENTIVES.load(deps.as_ref().storage, asset_denom).unwrap();
    assert_eq!(asset_incentive.index, expected_asset_incentive_index);
    assert_eq!(asset_incentive.last_updated, time_contract_call);

    let zero_incentive = ASSET_INCENTIVES.load(deps.as_ref().storage, zero_denom).unwrap();
    assert_eq!(zero_incentive.index, Decimal::one());
    assert_eq!(zero_incentive.last_updated, time_contract_call);

    let no_user_incentive = ASSET_INCENTIVES.load(deps.as_ref().storage, no_user_denom).unwrap();
    assert_eq!(no_user_incentive.index, Decimal::one());
    assert_eq!(no_user_incentive.last_updated, time_start);

    // user's asset and zero indices are updated
    let user_asset_index =
        USER_ASSET_INDICES.load(deps.as_ref().storage, (&user_addr, asset_denom)).unwrap();
    assert_eq!(user_asset_index, expected_asset_incentive_index);

    let user_zero_index =
        USER_ASSET_INDICES.load(deps.as_ref().storage, (&user_addr, zero_denom)).unwrap();
    assert_eq!(user_zero_index, Decimal::one());

    // user's no_user does not get updated
    let user_no_user_index =
        USER_ASSET_INDICES.may_load(deps.as_ref().storage, (&user_addr, no_user_denom)).unwrap();
    assert_eq!(user_no_user_index, None);

    // user rewards are cleared
    let user_unclaimed_rewards =
        USER_UNCLAIMED_REWARDS.load(deps.as_ref().storage, &user_addr).unwrap();
    assert_eq!(user_unclaimed_rewards, Uint128::zero())
}

#[test]
fn test_claim_zero_rewards() {
    // SETUP
    let mut deps = setup_test();

    let info = mock_info("user", &[]);
    let msg = ExecuteMsg::ClaimRewards {};

    let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);
    assert_eq!(
        res.attributes,
        vec![
            attr("action", "outposts/incentives/claim_rewards"),
            attr("user", "user"),
            attr("mars_rewards", "0"),
        ]
    );
}

#[test]
fn test_asset_incentive_compute_index() {
    assert_eq!(
        asset_incentive_compute_index(
            Decimal::zero(),
            Uint128::new(100),
            Uint128::new(200_000),
            1000,
            10
        ),
        Err(StdError::overflow(OverflowError::new(OverflowOperation::Sub, 1000, 10)))
    );

    assert_eq!(
        asset_incentive_compute_index(
            Decimal::zero(),
            Uint128::new(100),
            Uint128::new(200_000),
            0,
            1000
        )
        .unwrap(),
        Decimal::from_ratio(1_u128, 2_u128)
    );
    assert_eq!(
        asset_incentive_compute_index(
            Decimal::from_ratio(1_u128, 2_u128),
            Uint128::new(2000),
            Uint128::new(5_000_000),
            20_000,
            30_000
        )
        .unwrap(),
        Decimal::from_ratio(9_u128, 2_u128)
    );
}

#[test]
fn test_user_compute_accrued_rewards() {
    assert_eq!(
        user_compute_accrued_rewards(
            Uint128::zero(),
            Decimal::one(),
            Decimal::from_ratio(2_u128, 1_u128)
        )
        .unwrap(),
        Uint128::zero()
    );

    assert_eq!(
        user_compute_accrued_rewards(
            Uint128::new(100),
            Decimal::zero(),
            Decimal::from_ratio(2_u128, 1_u128)
        )
        .unwrap(),
        Uint128::new(200)
    );
    assert_eq!(
        user_compute_accrued_rewards(
            Uint128::new(100),
            Decimal::one(),
            Decimal::from_ratio(2_u128, 1_u128)
        )
        .unwrap(),
        Uint128::new(100)
    );
}