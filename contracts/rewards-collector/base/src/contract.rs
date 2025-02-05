use std::marker::PhantomData;

use cosmwasm_std::{
    to_binary, Addr, Binary, Coin, CosmosMsg, CustomMsg, CustomQuery, Deps, DepsMut, Env, IbcMsg,
    IbcTimeout, MessageInfo, Order, Response, StdResult, Uint128, WasmMsg,
};
use cw_storage_plus::{Bound, Item, Map};
use mars_owner::{Owner, OwnerInit::SetInitialOwner, OwnerUpdate};
use mars_red_bank_types::{
    address_provider::{self, MarsAddressType},
    incentives, red_bank,
    rewards_collector::{
        Config, ConfigResponse, ExecuteMsg, InstantiateMsg, QueryMsg, RouteResponse,
        RoutesResponse, UpdateConfig,
    },
};
use mars_utils::helpers::{option_string_to_addr, validate_native_denom};

use crate::{
    helpers::{stringify_option_amount, unwrap_option_amount},
    ContractError, ContractResult, Route,
};

const DEFAULT_LIMIT: u32 = 5;
const MAX_LIMIT: u32 = 10;

pub struct CollectorBase<'a, R, M, Q>
where
    R: Route<M, Q>,
    M: CustomMsg,
    Q: CustomQuery,
{
    /// Contract's owner
    pub owner: Owner<'a>,
    /// The contract's configurations
    pub config: Item<'a, Config>,
    /// The trade route for each pair of input/output assets
    pub routes: Map<'a, (String, String), R>,
    /// Phantom data that holds the custom message type
    pub custom_msg: PhantomData<M>,
    /// Phantom data that holds the custom query type
    pub custom_query: PhantomData<Q>,
}

impl<'a, R, M, Q> Default for CollectorBase<'a, R, M, Q>
where
    R: Route<M, Q>,
    M: CustomMsg,
    Q: CustomQuery,
{
    fn default() -> Self {
        Self {
            owner: Owner::new("owner"),
            config: Item::new("config"),
            routes: Map::new("routes"),
            custom_msg: PhantomData,
            custom_query: PhantomData,
        }
    }
}

impl<'a, R, M, Q> CollectorBase<'a, R, M, Q>
where
    R: Route<M, Q>,
    M: CustomMsg,
    Q: CustomQuery,
{
    pub fn instantiate(&self, deps: DepsMut<Q>, msg: InstantiateMsg) -> ContractResult<Response> {
        let owner = msg.owner.clone();

        let cfg = Config::checked(deps.api, msg)?;
        cfg.validate()?;

        self.owner.initialize(
            deps.storage,
            deps.api,
            SetInitialOwner {
                owner,
            },
        )?;

        self.config.save(deps.storage, &cfg)?;

        Ok(Response::default())
    }

    pub fn execute(
        &self,
        deps: DepsMut<Q>,
        env: Env,
        info: MessageInfo,
        msg: ExecuteMsg<R>,
    ) -> ContractResult<Response<M>> {
        match msg {
            ExecuteMsg::UpdateOwner(update) => self.update_owner(deps, info, update),
            ExecuteMsg::UpdateConfig {
                new_cfg,
            } => self.update_config(deps, info.sender, new_cfg),
            ExecuteMsg::SetRoute {
                denom_in,
                denom_out,
                route,
            } => self.set_route(deps, info.sender, denom_in, denom_out, route),
            ExecuteMsg::WithdrawFromRedBank {
                denom,
                amount,
            } => self.withdraw_from_red_bank(deps, denom, amount),
            ExecuteMsg::DistributeRewards {
                denom,
                amount,
            } => self.distribute_rewards(deps, env, denom, amount),
            ExecuteMsg::SwapAsset {
                denom,
                amount,
            } => self.swap_asset(deps, env, denom, amount),
            ExecuteMsg::ClaimIncentiveRewards {} => self.claim_incentive_rewards(deps),
        }
    }

    pub fn query(&self, deps: Deps<Q>, msg: QueryMsg) -> StdResult<Binary> {
        match msg {
            QueryMsg::Config {} => to_binary(&self.query_config(deps)?),
            QueryMsg::Route {
                denom_in,
                denom_out,
            } => to_binary(&self.query_route(deps, denom_in, denom_out)?),
            QueryMsg::Routes {
                start_after,
                limit,
            } => to_binary(&self.query_routes(deps, start_after, limit)?),
        }
    }

    fn update_owner(
        &self,
        deps: DepsMut<Q>,
        info: MessageInfo,
        update: OwnerUpdate,
    ) -> ContractResult<Response<M>> {
        Ok(self.owner.update(deps, info, update)?)
    }

    fn update_config(
        &self,
        deps: DepsMut<Q>,
        sender: Addr,
        new_cfg: UpdateConfig,
    ) -> ContractResult<Response<M>> {
        self.owner.assert_owner(deps.storage, &sender)?;

        let mut cfg = self.config.load(deps.storage)?;

        let UpdateConfig {
            address_provider,
            safety_tax_rate,
            safety_fund_denom,
            fee_collector_denom,
            channel_id,
            timeout_seconds,
            slippage_tolerance,
        } = new_cfg;

        cfg.address_provider =
            option_string_to_addr(deps.api, address_provider, cfg.address_provider)?;
        cfg.safety_tax_rate = safety_tax_rate.unwrap_or(cfg.safety_tax_rate);
        cfg.safety_fund_denom = safety_fund_denom.unwrap_or(cfg.safety_fund_denom);
        cfg.fee_collector_denom = fee_collector_denom.unwrap_or(cfg.fee_collector_denom);
        cfg.channel_id = channel_id.unwrap_or(cfg.channel_id);
        cfg.timeout_seconds = timeout_seconds.unwrap_or(cfg.timeout_seconds);
        cfg.slippage_tolerance = slippage_tolerance.unwrap_or(cfg.slippage_tolerance);

        cfg.validate()?;

        self.config.save(deps.storage, &cfg)?;

        Ok(Response::new().add_attribute("action", "mars/rewards-collector/update_config"))
    }

    fn set_route(
        &self,
        deps: DepsMut<Q>,
        sender: Addr,
        denom_in: String,
        denom_out: String,
        route: R,
    ) -> ContractResult<Response<M>> {
        self.owner.assert_owner(deps.storage, &sender)?;

        validate_native_denom(&denom_in)?;
        validate_native_denom(&denom_out)?;

        route.validate(&deps.querier, &denom_in, &denom_out)?;

        self.routes.save(deps.storage, (denom_in.clone(), denom_out.clone()), &route)?;

        Ok(Response::new()
            .add_attribute("action", "mars/rewards-collector/set_instructions")
            .add_attribute("denom_in", denom_in)
            .add_attribute("denom_out", denom_out)
            .add_attribute("route", route.to_string()))
    }

    fn withdraw_from_red_bank(
        &self,
        deps: DepsMut<Q>,
        denom: String,
        amount: Option<Uint128>,
    ) -> ContractResult<Response<M>> {
        let cfg = self.config.load(deps.storage)?;

        let red_bank_addr = address_provider::helpers::query_contract_addr(
            deps.as_ref(),
            &cfg.address_provider,
            MarsAddressType::RedBank,
        )?;

        let withdraw_msg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: red_bank_addr.to_string(),
            msg: to_binary(&red_bank::ExecuteMsg::Withdraw {
                denom: denom.clone(),
                amount,
                recipient: None,
            })?,
            funds: vec![],
        });

        Ok(Response::new()
            .add_message(withdraw_msg)
            .add_attribute("action", "withdraw_from_red_bank")
            .add_attribute("denom", denom)
            .add_attribute("amount", stringify_option_amount(amount)))
    }

    fn claim_incentive_rewards(&self, deps: DepsMut<Q>) -> ContractResult<Response<M>> {
        let cfg = self.config.load(deps.storage)?;

        let incentives_addr = address_provider::helpers::query_contract_addr(
            deps.as_ref(),
            &cfg.address_provider,
            MarsAddressType::Incentives,
        )?;

        let claim_msg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: incentives_addr.to_string(),
            msg: to_binary(&incentives::ExecuteMsg::ClaimRewards {})?,
            funds: vec![],
        });

        Ok(Response::new()
            .add_message(claim_msg)
            .add_attribute("action", "claim_incentive_rewards"))
    }

    fn swap_asset(
        &self,
        deps: DepsMut<Q>,
        env: Env,
        denom: String,
        amount: Option<Uint128>,
    ) -> ContractResult<Response<M>> {
        let cfg = self.config.load(deps.storage)?;

        // if amount is None, swap the total balance
        let amount_to_swap =
            unwrap_option_amount(&deps.querier, &env.contract.address, &denom, amount)?;

        // split the amount to swap between the safety fund and the fee collector
        let amount_safety_fund = amount_to_swap * cfg.safety_tax_rate;
        let amount_fee_collector = amount_to_swap.checked_sub(amount_safety_fund)?;
        let mut messages = vec![];

        // execute the swap to safety fund denom, if the amount to swap is non-zero,
        // and if the denom is not already the safety fund denom
        if !amount_safety_fund.is_zero() && denom != cfg.safety_fund_denom {
            messages.push(
                self.routes
                    .load(deps.storage, (denom.clone(), cfg.safety_fund_denom))?
                    .build_swap_msg(
                        &env,
                        &deps.querier,
                        &denom,
                        amount_safety_fund,
                        cfg.slippage_tolerance,
                    )?,
            );
        }

        // execute the swap to fee collector denom, if the amount to swap is non-zero,
        // and if the denom is not already the fee collector denom
        if !amount_fee_collector.is_zero() && denom != cfg.fee_collector_denom {
            messages.push(
                self.routes
                    .load(deps.storage, (denom.clone(), cfg.fee_collector_denom))?
                    .build_swap_msg(
                        &env,
                        &deps.querier,
                        &denom,
                        amount_fee_collector,
                        cfg.slippage_tolerance,
                    )?,
            );
        }

        Ok(Response::new()
            .add_messages(messages)
            .add_attribute("action", "swap_asset")
            .add_attribute("denom", denom)
            .add_attribute("amount_safety_fund", amount_safety_fund)
            .add_attribute("amount_fee_collector", amount_fee_collector)
            .add_attribute("slippage_tolerance", cfg.slippage_tolerance.to_string()))
    }

    fn distribute_rewards(
        &self,
        deps: DepsMut<Q>,
        env: Env,
        denom: String,
        amount: Option<Uint128>,
    ) -> ContractResult<Response<M>> {
        let cfg = self.config.load(deps.storage)?;

        let to_address = if denom == cfg.safety_fund_denom {
            address_provider::helpers::query_module_addr(
                deps.as_ref(),
                &cfg.address_provider,
                MarsAddressType::SafetyFund,
            )?
        } else if denom == cfg.fee_collector_denom {
            address_provider::helpers::query_module_addr(
                deps.as_ref(),
                &cfg.address_provider,
                MarsAddressType::FeeCollector,
            )?
        } else {
            return Err(ContractError::AssetNotEnabledForDistribution {
                denom,
            });
        };

        let amount_to_distribute =
            unwrap_option_amount(&deps.querier, &env.contract.address, &denom, amount)?;

        let transfer_msg = CosmosMsg::Ibc(IbcMsg::Transfer {
            channel_id: cfg.channel_id,
            to_address: to_address.to_string(),
            amount: Coin {
                denom: denom.clone(),
                amount: amount_to_distribute,
            },
            timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(cfg.timeout_seconds)),
        });

        Ok(Response::new()
            .add_message(transfer_msg)
            .add_attribute("action", "distribute_rewards")
            .add_attribute("denom", denom)
            .add_attribute("amount", amount_to_distribute)
            .add_attribute("to", to_address))
    }

    fn query_config(&self, deps: Deps<Q>) -> StdResult<ConfigResponse> {
        let owner_state = self.owner.query(deps.storage)?;
        let cfg = self.config.load(deps.storage)?;
        Ok(ConfigResponse {
            owner: owner_state.owner,
            proposed_new_owner: owner_state.proposed,
            address_provider: cfg.address_provider.into(),
            safety_tax_rate: cfg.safety_tax_rate,
            safety_fund_denom: cfg.safety_fund_denom,
            fee_collector_denom: cfg.fee_collector_denom,
            channel_id: cfg.channel_id,
            timeout_seconds: cfg.timeout_seconds,
            slippage_tolerance: cfg.slippage_tolerance,
        })
    }

    fn query_route(
        &self,
        deps: Deps<Q>,
        denom_in: String,
        denom_out: String,
    ) -> StdResult<RouteResponse<R>> {
        Ok(RouteResponse {
            denom_in: denom_in.clone(),
            denom_out: denom_out.clone(),
            route: self.routes.load(deps.storage, (denom_in, denom_out))?,
        })
    }

    fn query_routes(
        &self,
        deps: Deps<Q>,
        start_after: Option<(String, String)>,
        limit: Option<u32>,
    ) -> StdResult<RoutesResponse<R>> {
        let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
        let start = start_after.map(Bound::exclusive);

        self.routes
            .range(deps.storage, start, None, Order::Ascending)
            .take(limit)
            .map(|item| {
                let (k, v) = item?;
                Ok(RouteResponse {
                    denom_in: k.0,
                    denom_out: k.1,
                    route: v,
                })
            })
            .collect()
    }
}
