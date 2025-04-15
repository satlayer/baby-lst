use cosmos_sdk_proto::cosmos::staking::v1beta1::MsgDelegate;
use cosmwasm_std::{
    attr, to_json_binary, CosmosMsg, DepsMut, Env, Event, MessageInfo, QueryRequest, Response,
    Uint128, WasmMsg, WasmQuery,
};
use cw20_base::msg::ExecuteMsg as Cw20ExecuteMsg;

use lst_common::{
    babylon_msg::{CosmosAny, MsgWrappedDelegate},
    calculate_delegations,
    errors::HubError,
    types::{LstResult, ProtoCoin, ResponseType},
    validator::{QueryMsg::ValidatorsDelegation, ValidatorResponse},
    ContractError, ValidatorError,
};

use crate::{
    contract::{check_slashing, query_total_lst_token_issued},
    math::decimal_division,
    state::{
        update_pending_delegation_amount, update_state, StakeType, CONFIG, CURRENT_BATCH,
        PARAMETERS, STATE,
    },
};

pub fn execute_stake(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    stake_type: StakeType,
) -> LstResult<Response<ResponseType>> {
    let params = PARAMETERS.load(deps.storage)?;
    let staking_coin_denom = params.staking_coin_denom;

    let config = CONFIG.load(deps.storage)?;
    let sender = info.sender.clone();

    let mut state = STATE.load(deps.storage)?;
    let old_state = state.clone();

    let reward_dispatcher_address = &config
        .reward_dispatcher_contract
        .ok_or(HubError::RewardDispatcherNotSet)?;

    //If stake type is StakeRewards, we need to check if the sender is the reward dispatcher contract
    if stake_type == StakeType::StakeRewards && sender != reward_dispatcher_address {
        return Err(ContractError::Unauthorized {});
    }

    let current_batch = CURRENT_BATCH.load(deps.storage)?;
    let requested_withdrawal_amount = current_batch.requested_lst_token_amount;

    if info.funds.len() > 1usize {
        return Err(HubError::OnlyOneCoinAllowed.into());
    }

    let payment = info
        .funds
        .iter()
        .find(|coin| coin.denom == staking_coin_denom && coin.amount > Uint128::zero())
        .ok_or(HubError::InvalidAmount)?;

    let mut events: Vec<Event> = vec![];
    // check slashing and get the latest exchange rate
    let (slashing_events, _) = check_slashing(&mut deps, &env, &mut state)?;
    events.extend(slashing_events);

    let mut total_supply = query_total_lst_token_issued(deps.as_ref()).unwrap();

    let mint_amount = match stake_type {
        StakeType::LSTMint => decimal_division(payment.amount, state.lst_exchange_rate),
        StakeType::StakeRewards => Uint128::zero(),
    };

    total_supply += mint_amount;

    // state update
    match stake_type {
        StakeType::LSTMint => {
            state.total_staked_amount += payment.amount;
            state.update_lst_exchange_rate(total_supply, requested_withdrawal_amount);
        }
        StakeType::StakeRewards => {
            state.total_staked_amount += payment.amount;
            state.update_lst_exchange_rate(total_supply, requested_withdrawal_amount);
        }
    }
    update_pending_delegation_amount(&mut deps, &env, Some(payment.amount), None)?;
    let state_events = update_state(deps.storage, old_state, state)?;
    events.extend(state_events);

    //validators management
    let validators_registry_contract = config
        .validators_registry_contract
        .ok_or(HubError::ValidatorRegistryNotSet)?;

    let validators: Vec<ValidatorResponse> =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: validators_registry_contract.to_string(),
            msg: to_json_binary(&ValidatorsDelegation {})?,
        }))?;

    if validators.is_empty() {
        return Err(ValidatorError::EmptyValidatorSet.into());
    }

    let delegations = calculate_delegations(payment.amount, validators.as_slice())?;

    let mut external_call_msgs: Vec<CosmosMsg> = vec![];
    for i in 0..delegations.len() {
        if delegations[i].is_zero() {
            continue;
        }

        let msg = prepare_wrapped_delegate_msg(
            payment.denom.to_string(),
            delegations[i].to_string(),
            env.contract.address.to_string(),
            validators[i].address.to_string(),
        );

        external_call_msgs.push(msg);
    }

    //Skip minting of lst token in case of staking rewards
    if stake_type == StakeType::StakeRewards {
        let res = Response::new()
            .add_messages(external_call_msgs)
            .add_events(events)
            .add_attributes(vec![
                attr("action", "stake_rewards"),
                attr("from", sender.clone()),
                attr("amount", payment.amount.to_string()),
            ]);
        return Ok(res);
    }

    // Create mint message
    let mint_msg = Cw20ExecuteMsg::Mint {
        recipient: sender.to_string(),
        amount: mint_amount,
    };

    let token_address = config.lst_token.ok_or(HubError::LstTokenNotSet)?;

    external_call_msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: token_address.to_string(),
        msg: to_json_binary(&mint_msg)?,
        funds: vec![],
    }));

    let res = Response::new()
        .add_messages(external_call_msgs)
        .add_events(events)
        .add_attributes(vec![
            attr("action", "mint"),
            attr("from", sender.clone()),
            attr("staked", payment.amount),
            attr("minted", mint_amount),
        ]);

    Ok(res)
}

fn prepare_wrapped_delegate_msg(
    denom: String,
    amount: String,
    delegator_address: String,
    validator_address: String,
) -> CosmosMsg {
    let coin = ProtoCoin { denom, amount };

    let delegate_msg = MsgDelegate {
        delegator_address,
        validator_address,
        amount: Some(coin),
    };

    MsgWrappedDelegate {
        msg: Some(delegate_msg),
    }
    .to_any()
}

#[cfg(test)]
mod tests {
    use cosmos_sdk_proto::{cosmos::staking::v1beta1::MsgDelegate, traits::MessageExt};
    use cosmwasm_std::{
        attr, coin, from_json,
        testing::{message_info, mock_dependencies, mock_env},
        to_json_binary, AnyMsg, Binary, ContractResult, CosmosMsg, SubMsg, SystemResult, Uint128,
        WasmMsg, WasmQuery,
    };
    use cw20::{Cw20ExecuteMsg, Cw20QueryMsg};
    use cw20_base::state::TokenInfo;
    use lst_common::{
        babylon_msg::MsgWrappedDelegate,
        errors::HubError,
        hub::InstantiateMsg,
        types::ProtoCoin,
        validator::{
            QueryMsg::{self as ValidatorQueryMsg, ValidatorsDelegation},
            ValidatorResponse,
        },
        ContractError,
    };

    use crate::{config::execute_update_config, instantiate, state::StakeType};

    use super::execute_stake;

    #[test]
    fn test_execute_stake() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let owner = deps.api.addr_make("owner");
        let denom = "denom";
        let info = message_info(&owner, &[]);

        // instantiate successfully
        {
            let msg = InstantiateMsg {
                epoch_length: 7200,
                staking_coin_denom: denom.to_string(),
                unstaking_period: 10000,
                staking_epoch_start_block_height: 100,
                staking_epoch_length_blocks: 360,
            };

            instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        }

        // RewardDispatcherNotSet error
        {
            let err = execute_stake(deps.as_mut(), env.clone(), info.clone(), StakeType::LSTMint)
                .unwrap_err();
            assert_eq!(err, ContractError::Hub(HubError::RewardDispatcherNotSet));
        }

        // Unauthorized error
        {
            let reward_dispatcher = deps.api.addr_make("reward_dispatcher");
            execute_update_config(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                None,
                None,
                None,
                Some(reward_dispatcher.to_string()),
            )
            .unwrap();

            let err = execute_stake(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                StakeType::StakeRewards,
            )
            .unwrap_err();
            assert_eq!(err, ContractError::Unauthorized {});
        }

        // Unauthorized error
        {
            let reward_dispatcher = deps.api.addr_make("reward_dispatcher");
            execute_update_config(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                None,
                None,
                None,
                Some(reward_dispatcher.to_string()),
            )
            .unwrap();

            let err = execute_stake(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                StakeType::StakeRewards,
            )
            .unwrap_err();
            assert_eq!(err, ContractError::Unauthorized {});
        }

        // OnlyOneCoinAllowed error
        {
            let reward_dispatcher = deps.api.addr_make("reward_dispatcher");
            let info = message_info(&owner, &[coin(100, denom), coin(100, "denom1")]);
            execute_update_config(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                None,
                None,
                None,
                Some(reward_dispatcher.to_string()),
            )
            .unwrap();

            let err = execute_stake(deps.as_mut(), env.clone(), info.clone(), StakeType::LSTMint)
                .unwrap_err();
            assert_eq!(err, ContractError::Hub(HubError::OnlyOneCoinAllowed));
        }

        // InvalidAmount error
        {
            let reward_dispatcher = deps.api.addr_make("reward_dispatcher");
            execute_update_config(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                None,
                None,
                None,
                Some(reward_dispatcher.to_string()),
            )
            .unwrap();

            let err = execute_stake(deps.as_mut(), env.clone(), info.clone(), StakeType::LSTMint)
                .unwrap_err();
            assert_eq!(err, ContractError::Hub(HubError::InvalidAmount));
        }

        // LstTokenNotSet error
        {
            let reward_dispatcher = deps.api.addr_make("reward_dispatcher");
            let validator_registry = deps.api.addr_make("validator_registry");
            let validator_registry_clone = validator_registry.clone();

            let lst_token = deps.api.addr_make("lst_token");
            let lst_token_clone = lst_token.clone();

            deps.querier.update_wasm(move |query| match query {
                WasmQuery::Smart { contract_addr, msg } => {
                    if contract_addr.to_string() == lst_token_clone.to_string() {
                        let msg: Cw20QueryMsg = from_json(msg).unwrap();
                        match msg {
                            Cw20QueryMsg::TokenInfo {} => SystemResult::Ok(ContractResult::Ok(
                                to_json_binary(&TokenInfo {
                                    name: "Test".to_string(),
                                    symbol: "TT".to_string(),
                                    decimals: 6,
                                    total_supply: Uint128::new(1000),
                                    mint: None,
                                })
                                .unwrap(),
                            )),
                            _ => panic!("unexpected query"),
                        }
                    } else if contract_addr.to_string() == validator_registry_clone.to_string() {
                        let msg: ValidatorQueryMsg = from_json(msg).unwrap();
                        match msg {
                            ValidatorsDelegation {} => SystemResult::Ok(ContractResult::Ok(
                                to_json_binary(&vec![ValidatorResponse {
                                    total_delegated: Uint128::new(100),
                                    address: "validator1".to_string(),
                                }])
                                .unwrap(),
                            )),
                            _ => panic!("unexpected query"),
                        }
                    } else {
                        panic!("unexpected query")
                    }
                }
                _ => SystemResult::Err(cosmwasm_std::SystemError::Unknown {}),
            });

            execute_update_config(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                None,
                Some(lst_token.to_string()),
                Some(validator_registry.to_string()),
                Some(reward_dispatcher.to_string()),
            )
            .unwrap();

            let info = message_info(&owner, &[coin(100, denom)]);
            let response =
                execute_stake(deps.as_mut(), env.clone(), info.clone(), StakeType::LSTMint)
                    .unwrap();

            assert_eq!(
                response.messages,
                vec![
                    SubMsg::new(CosmosMsg::Any(AnyMsg {
                        type_url: "/babylon.epoching.v1.MsgWrappedDelegate".to_string(),
                        value: Binary::from(
                            MsgWrappedDelegate {
                                msg: Some(MsgDelegate {
                                    delegator_address: env.contract.address.to_string(),
                                    validator_address: "validator1".to_string(),
                                    amount: Some(ProtoCoin {
                                        denom: denom.to_string(),
                                        amount: "100".to_string()
                                    })
                                })
                            }
                            .to_bytes()
                            .unwrap()
                        )
                    })),
                    SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: lst_token.to_string(),
                        msg: to_json_binary(&Cw20ExecuteMsg::Mint {
                            recipient: owner.to_string(),
                            amount: Uint128::new(100)
                        })
                        .unwrap(),
                        funds: vec![]
                    }))
                ]
            );

            assert_eq!(
                response.attributes,
                vec![
                    attr("action", "mint"),
                    attr("from", owner.to_string()),
                    attr("staked", "100"),
                    attr("minted", "100"),
                ]
            );
        }
    }
}
