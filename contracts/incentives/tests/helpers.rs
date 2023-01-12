#![allow(dead_code)]

use cosmwasm_std::{
    testing::{mock_env, mock_info, MockApi, MockStorage},
    Env, OwnedDeps,
};
use mars_incentives::contract::instantiate;
use mars_outpost::incentives::InstantiateMsg;
use mars_testing::{mock_dependencies, MarsMockQuerier};

pub fn setup_test() -> OwnedDeps<MockStorage, MockApi, MarsMockQuerier> {
    setup_test_with_env(mock_env())
}

pub fn setup_test_with_env(env: Env) -> OwnedDeps<MockStorage, MockApi, MarsMockQuerier> {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        owner: String::from("owner"),
        address_provider: String::from("address_provider"),
        mars_denom: String::from("umars"),
    };
    let info = mock_info("owner", &[]);
    instantiate(deps.as_mut(), env, info, msg).unwrap();

    deps
}
