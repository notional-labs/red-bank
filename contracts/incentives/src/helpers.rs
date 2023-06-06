use std::cmp::{max, min};

use cosmwasm_std::{
    Addr, BlockInfo, Decimal, Deps, DepsMut, OverflowError, OverflowOperation, StdError, StdResult,
    Uint128,
};
use mars_red_bank_types::{incentives::AssetIncentive, red_bank};

use crate::state::{ASSET_INCENTIVES, USER_ASSET_INDICES, USER_UNCLAIMED_REWARDS};

/// Updates asset incentive index and last updated timestamp by computing
/// how many rewards were accrued since last time updated given incentive's
/// emission per second.
/// Total supply is the total (liquidity) token supply during the period being computed.
/// Note that this method does not commit updates to state as that should be executed by the
/// caller
pub fn update_asset_incentive_index(
    asset_incentive: &mut AssetIncentive,
    total_amount_scaled: Uint128,
    current_block_time: u64,
) -> StdResult<()> {
    let end_time_sec = asset_incentive.start_time + asset_incentive.duration;
    if (current_block_time != asset_incentive.last_updated)
        && current_block_time > asset_incentive.start_time
        && asset_incentive.last_updated < end_time_sec
        && !total_amount_scaled.is_zero()
        && !asset_incentive.emission_per_second.is_zero()
    {
        let time_start = max(asset_incentive.start_time, asset_incentive.last_updated);
        let time_end = min(current_block_time, end_time_sec);
        asset_incentive.index = compute_asset_incentive_index(
            asset_incentive.index,
            asset_incentive.emission_per_second,
            total_amount_scaled,
            time_start,
            time_end,
        )?;
    }
    asset_incentive.last_updated = current_block_time;
    Ok(())
}

pub fn compute_asset_incentive_index(
    previous_index: Decimal,
    emission_per_second: Uint128,
    total_amount_scaled: Uint128,
    time_start: u64,
    time_end: u64,
) -> StdResult<Decimal> {
    if time_start > time_end {
        return Err(StdError::overflow(OverflowError::new(
            OverflowOperation::Sub,
            time_start,
            time_end,
        )));
    }
    let seconds_elapsed = time_end - time_start;
    let emission_for_elapsed_seconds =
        emission_per_second.checked_mul(Uint128::from(seconds_elapsed))?;
    let new_index =
        previous_index + Decimal::from_ratio(emission_for_elapsed_seconds, total_amount_scaled);
    Ok(new_index)
}

/// Computes user accrued rewards using the difference between asset_incentive index and
/// user current index
/// asset_incentives index should be up to date.
pub fn compute_user_accrued_rewards(
    user_amount_scaled: Uint128,
    user_asset_index: Decimal,
    asset_incentive_index: Decimal,
) -> StdResult<Uint128> {
    let result = (user_amount_scaled * asset_incentive_index)
        .checked_sub(user_amount_scaled * user_asset_index)?;
    Ok(result)
}

/// Result of querying and updating the status of the user and a give asset incentives in order to
/// compute unclaimed rewards.
pub struct UserAssetIncentiveStatus {
    /// Current user index's value on the contract store (not updated by current asset index)
    pub user_index_current: Decimal,
    /// Asset incentive with values updated to the current block (not neccesarily commited
    /// to storage)
    pub asset_incentive_updated: AssetIncentive,
}

pub struct Collaterals {
    pub user_collateral: Uint128,
    pub total_collateral: Uint128,
}

pub fn compute_new_user_unclaimed_rewards(
    deps: Deps,
    block: &BlockInfo,
    red_bank_addr: &Addr,
    user_addr: &Addr,
    collateral_denom: &str,
    incentive_denom: &str,
    mut asset_incentive: AssetIncentive,
    collaterals: Option<Collaterals>,
) -> StdResult<(Uint128, Option<UserAssetIncentiveStatus>)> {
    // Get asset user balances and total supply
    let (user_collateral, total_collateral) = collaterals
        .map(|x| Ok::<_, StdError>((x.user_collateral, x.total_collateral)))
        .unwrap_or_else(|| {
            let collateral: red_bank::UserCollateralResponse = deps.querier.query_wasm_smart(
                red_bank_addr,
                &red_bank::QueryMsg::UserCollateral {
                    user: user_addr.to_string(),
                    denom: collateral_denom.to_string(),
                },
            )?;
            let market: red_bank::Market = deps.querier.query_wasm_smart(
                red_bank_addr,
                &red_bank::QueryMsg::Market {
                    denom: collateral_denom.to_string(),
                },
            )?;
            Ok((collateral.amount_scaled, market.collateral_total_scaled))
        })?;

    // If user's balance is 0 there should be no rewards to accrue, so we don't care about
    // updating indexes. If the user's balance changes, the indexes will be updated correctly at
    // that point in time.
    if user_collateral.is_zero() {
        return Ok((Uint128::zero(), None));
    }

    update_asset_incentive_index(&mut asset_incentive, total_collateral, block.time.seconds())?;

    let user_asset_index = USER_ASSET_INDICES
        .may_load(deps.storage, (user_addr, &collateral_denom, &incentive_denom))?
        .unwrap_or_else(Decimal::zero);

    let mut unclaimed_rewards = Uint128::zero();

    if user_asset_index != asset_incentive.index {
        // Compute user accrued rewards and update user index
        let asset_accrued_rewards =
            compute_user_accrued_rewards(user_collateral, user_asset_index, asset_incentive.index)?;
        unclaimed_rewards = asset_accrued_rewards;
    }

    let user_asset_incentive_status_to_update = UserAssetIncentiveStatus {
        user_index_current: user_asset_index,
        asset_incentive_updated: asset_incentive,
    };

    Ok((unclaimed_rewards, Some(user_asset_incentive_status_to_update)))
}

pub fn update_user_rewards(
    deps: DepsMut,
    block: &BlockInfo,
    red_bank_addr: &Addr,
    user_addr: &Addr,
    collateral_denom: &str,
    incentive_denom: &str,
    asset_incentive: &AssetIncentive,
    collaterals: Option<Collaterals>,
    clear_rewards: bool,
) -> StdResult<(Uint128, Uint128, AssetIncentive)> {
    let previous_rewards = USER_UNCLAIMED_REWARDS
        .may_load(deps.storage, (user_addr, &collateral_denom, &incentive_denom))?
        .unwrap_or_else(Uint128::zero);

    let (new_unclaimed_rewards, user_asset_incentive_status) = compute_new_user_unclaimed_rewards(
        deps.as_ref(),
        block,
        red_bank_addr,
        user_addr,
        collateral_denom,
        incentive_denom,
        asset_incentive.clone(),
        collaterals,
    )?;

    // Save new rewards
    let current_rewards = previous_rewards + new_unclaimed_rewards;
    if clear_rewards {
        USER_UNCLAIMED_REWARDS.save(
            deps.storage,
            (user_addr, &collateral_denom, &incentive_denom),
            &Uint128::zero(),
        )?;
    } else {
        USER_UNCLAIMED_REWARDS.save(
            deps.storage,
            (user_addr, &collateral_denom, &incentive_denom),
            &current_rewards,
        )?;
    }

    // Update user index and asset incentive index if necessary
    let new_asset_incentive = if let Some(user_asset_incentive_status) = user_asset_incentive_status
    {
        let asset_incentive_updated = user_asset_incentive_status.asset_incentive_updated;
        ASSET_INCENTIVES.save(
            deps.storage,
            (collateral_denom.to_string(), incentive_denom.to_string()),
            &asset_incentive_updated,
        )?;

        if asset_incentive_updated.index != user_asset_incentive_status.user_index_current {
            USER_ASSET_INDICES.save(
                deps.storage,
                (&user_addr, &collateral_denom, &incentive_denom),
                &asset_incentive_updated.index,
            )?;
        }
        asset_incentive_updated
    } else {
        asset_incentive.clone()
    };

    Ok((previous_rewards, current_rewards, new_asset_incentive))
}
