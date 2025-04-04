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

    let mut total_supply = query_total_lst_token_issued(deps.as_ref()).unwrap_or_default();

    let mint_amount = match stake_type {
        StakeType::LSTMint => decimal_division(payment.amount, state.lst_exchange_rate),
        StakeType::StakeRewards => Uint128::zero(),
    };

    total_supply += mint_amount;

    // state update
    match stake_type {
        StakeType::LSTMint => {
            state.total_staked_amount += payment.amount;
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
