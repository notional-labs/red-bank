// @ts-nocheck
/**
 * This file was automatically generated by @cosmwasm/ts-codegen@0.30.0.
 * DO NOT MODIFY IT BY HAND. Instead, modify the source JSONSchema file,
 * and run the @cosmwasm/ts-codegen generate command to regenerate this file.
 */

export type Decimal = string
export interface InstantiateMsg {
  address_provider: string
  channel_id: string
  fee_collector_denom: string
  owner: string
  safety_fund_denom: string
  safety_tax_rate: Decimal
  slippage_tolerance: Decimal
  timeout_seconds: number
}
export type ExecuteMsg =
  | {
      update_owner: OwnerUpdate
    }
  | {
      update_config: {
        new_cfg: UpdateConfig
      }
    }
  | {
      set_route: {
        denom_in: string
        denom_out: string
        route: OsmosisRoute
      }
    }
  | {
      withdraw_from_red_bank: {
        amount?: Uint128 | null
        denom: string
      }
    }
  | {
      distribute_rewards: {
        amount?: Uint128 | null
        denom: string
      }
    }
  | {
      swap_asset: {
        amount?: Uint128 | null
        denom: string
      }
    }
  | {
      claim_incentive_rewards: {}
    }
export type OwnerUpdate =
  | {
      propose_new_owner: {
        proposed: string
      }
    }
  | 'clear_proposed'
  | 'accept_proposed'
  | 'abolish_owner_role'
  | {
      set_emergency_owner: {
        emergency_owner: string
      }
    }
  | 'clear_emergency_owner'
export type OsmosisRoute = SwapAmountInRoute[]
export type Uint128 = string
export interface UpdateConfig {
  address_provider?: string | null
  channel_id?: string | null
  fee_collector_denom?: string | null
  safety_fund_denom?: string | null
  safety_tax_rate?: Decimal | null
  slippage_tolerance?: Decimal | null
  timeout_seconds?: number | null
}
export interface SwapAmountInRoute {
  pool_id: number
  token_out_denom: string
  [k: string]: unknown
}
export type QueryMsg =
  | {
      config: {}
    }
  | {
      route: {
        denom_in: string
        denom_out: string
      }
    }
  | {
      routes: {
        limit?: number | null
        start_after?: [string, string] | null
      }
    }
export interface ConfigResponse {
  address_provider: string
  channel_id: string
  fee_collector_denom: string
  owner?: string | null
  proposed_new_owner?: string | null
  safety_fund_denom: string
  safety_tax_rate: Decimal
  slippage_tolerance: Decimal
  timeout_seconds: number
}
export interface RouteResponseForString {
  denom_in: string
  denom_out: string
  route: string
}
export type ArrayOfRouteResponseForString = RouteResponseForString[]
