use cosmwasm_std::{
    from_binary, from_slice,
    testing::{MockQuerier, MOCK_CONTRACT_ADDR},
    Addr, Coin, Decimal, Empty, Querier, QuerierResult, QueryRequest, StdResult, SystemError,
    SystemResult, Uint128, WasmQuery,
};
use mars_oracle_osmosis::{
    stride,
    stride::{Price, RedemptionRateResponse},
    DowntimeDetector,
};
use mars_osmosis::helpers::QueryPoolResponse;
use mars_red_bank_types::{address_provider, incentives, oracle, red_bank};
use osmosis_std::types::osmosis::{
    downtimedetector::v1beta1::RecoveredSinceDowntimeOfLengthResponse,
    poolmanager::v1beta1::SpotPriceResponse,
    twap::v1beta1::{ArithmeticTwapToNowResponse, GeometricTwapToNowResponse},
};
use pyth_sdk_cw::{PriceFeedResponse, PriceIdentifier};

use crate::{
    incentives_querier::IncentivesQuerier,
    mock_address_provider,
    oracle_querier::OracleQuerier,
    osmosis_querier::{OsmosisQuerier, PriceKey},
    pyth_querier::PythQuerier,
    red_bank_querier::RedBankQuerier,
    redemption_rate_querier::RedemptionRateQuerier,
};

pub struct MarsMockQuerier {
    base: MockQuerier<Empty>,
    oracle_querier: OracleQuerier,
    incentives_querier: IncentivesQuerier,
    osmosis_querier: OsmosisQuerier,
    pyth_querier: PythQuerier,
    redbank_querier: RedBankQuerier,
    redemption_rate_querier: RedemptionRateQuerier,
}

impl Querier for MarsMockQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        let request: QueryRequest<Empty> = match from_slice(bin_request) {
            Ok(v) => v,
            Err(e) => {
                return SystemResult::Err(SystemError::InvalidRequest {
                    error: format!("Parsing query request: {e}"),
                    request: bin_request.into(),
                })
            }
        };

        self.handle_query(&request)
    }
}

impl MarsMockQuerier {
    pub fn new(base: MockQuerier<Empty>) -> Self {
        MarsMockQuerier {
            base,
            oracle_querier: OracleQuerier::default(),
            incentives_querier: IncentivesQuerier::default(),
            osmosis_querier: OsmosisQuerier::default(),
            pyth_querier: PythQuerier::default(),
            redbank_querier: RedBankQuerier::default(),
            redemption_rate_querier: Default::default(),
        }
    }

    /// Set new balances for contract address
    pub fn set_contract_balances(&mut self, contract_balances: &[Coin]) {
        let contract_addr = Addr::unchecked(MOCK_CONTRACT_ADDR);
        self.base.update_balance(contract_addr.to_string(), contract_balances.to_vec());
    }

    pub fn set_oracle_price(&mut self, denom: &str, price: Decimal) {
        self.oracle_querier.prices.insert(denom.to_string(), price);
    }

    pub fn set_incentives_address(&mut self, address: Addr) {
        self.incentives_querier.incentives_addr = address;
    }

    pub fn set_unclaimed_rewards(&mut self, user_address: String, unclaimed_rewards: Uint128) {
        self.incentives_querier
            .unclaimed_rewards_at
            .insert(Addr::unchecked(user_address), unclaimed_rewards);
    }

    pub fn set_query_pool_response(&mut self, pool_id: u64, pool_response: QueryPoolResponse) {
        self.osmosis_querier.pools.insert(pool_id, pool_response);
    }

    pub fn set_spot_price(
        &mut self,
        id: u64,
        base_asset_denom: &str,
        quote_asset_denom: &str,
        spot_price: SpotPriceResponse,
    ) {
        let price_key = PriceKey {
            pool_id: id,
            denom_in: base_asset_denom.to_string(),
            denom_out: quote_asset_denom.to_string(),
        };
        self.osmosis_querier.spot_prices.insert(price_key, spot_price);
    }

    pub fn set_arithmetic_twap_price(
        &mut self,
        id: u64,
        base_asset_denom: &str,
        quote_asset_denom: &str,
        twap_price: ArithmeticTwapToNowResponse,
    ) {
        let price_key = PriceKey {
            pool_id: id,
            denom_in: base_asset_denom.to_string(),
            denom_out: quote_asset_denom.to_string(),
        };
        self.osmosis_querier.arithmetic_twap_prices.insert(price_key, twap_price);
    }

    pub fn set_geometric_twap_price(
        &mut self,
        id: u64,
        base_asset_denom: &str,
        quote_asset_denom: &str,
        twap_price: GeometricTwapToNowResponse,
    ) {
        let price_key = PriceKey {
            pool_id: id,
            denom_in: base_asset_denom.to_string(),
            denom_out: quote_asset_denom.to_string(),
        };
        self.osmosis_querier.geometric_twap_prices.insert(price_key, twap_price);
    }

    pub fn set_downtime_detector(&mut self, downtime_detector: DowntimeDetector, recovered: bool) {
        self.osmosis_querier.downtime_detector.insert(
            (downtime_detector.downtime as i32, downtime_detector.recovery),
            RecoveredSinceDowntimeOfLengthResponse {
                succesfully_recovered: recovered,
            },
        );
    }

    pub fn set_pyth_price(&mut self, id: PriceIdentifier, price: PriceFeedResponse) {
        self.pyth_querier.prices.insert(id, price);
    }

    pub fn set_redbank_market(&mut self, market: red_bank::Market) {
        self.redbank_querier.markets.insert(market.denom.clone(), market);
    }

    pub fn set_red_bank_user_collateral(
        &mut self,
        user: impl Into<String>,
        collateral: red_bank::UserCollateralResponse,
    ) {
        self.redbank_querier
            .users_denoms_collaterals
            .insert((user.into(), collateral.denom.clone()), collateral);
    }

    pub fn set_redbank_user_position(
        &mut self,
        user_address: String,
        position: red_bank::UserPositionResponse,
    ) {
        self.redbank_querier.users_positions.insert(user_address, position);
    }

    pub fn set_redemption_rate(
        &mut self,
        denom: &str,
        base_denom: &str,
        redemption_rate: RedemptionRateResponse,
    ) {
        let price_key = Price {
            denom: denom.to_string(),
            base_denom: base_denom.to_string(),
        };
        self.redemption_rate_querier.redemption_rates.insert(price_key, redemption_rate);
    }

    pub fn handle_query(&self, request: &QueryRequest<Empty>) -> QuerierResult {
        match &request {
            QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr,
                msg,
            }) => {
                let contract_addr = Addr::unchecked(contract_addr);

                // Address Provider Queries
                let parse_address_provider_query: StdResult<address_provider::QueryMsg> =
                    from_binary(msg);
                if let Ok(address_provider_query) = parse_address_provider_query {
                    return mock_address_provider::handle_query(
                        &contract_addr,
                        address_provider_query,
                    );
                }

                // Oracle Queries
                let parse_oracle_query: StdResult<oracle::QueryMsg> = from_binary(msg);
                if let Ok(oracle_query) = parse_oracle_query {
                    return self.oracle_querier.handle_query(&contract_addr, oracle_query);
                }

                // Incentives Queries
                let parse_incentives_query: StdResult<incentives::QueryMsg> = from_binary(msg);
                if let Ok(incentives_query) = parse_incentives_query {
                    return self.incentives_querier.handle_query(&contract_addr, incentives_query);
                }

                // Pyth Queries
                if let Ok(pyth_query) = from_binary::<pyth_sdk_cw::QueryMsg>(msg) {
                    return self.pyth_querier.handle_query(&contract_addr, pyth_query);
                }

                // RedBank Queries
                if let Ok(redbank_query) = from_binary::<red_bank::QueryMsg>(msg) {
                    return self.redbank_querier.handle_query(redbank_query);
                }

                // Redemption Rate Queries
                if let Ok(redemption_rate_req) = from_binary::<stride::RedemptionRateRequest>(msg) {
                    return self.redemption_rate_querier.handle_query(redemption_rate_req);
                }

                panic!("[mock]: Unsupported wasm query: {msg:?}");
            }

            QueryRequest::Stargate {
                path,
                data,
            } => {
                if let Ok(querier_res) = self.osmosis_querier.handle_stargate_query(path, data) {
                    return querier_res;
                }

                panic!("[mock]: Unsupported stargate query, path: {path:?}");
            }

            _ => self.base.handle_query(request),
        }
    }
}
