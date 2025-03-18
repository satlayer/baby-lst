use crate::{
    contract::{calculate_delegations, check_slashing, query_total_lst_token_issued},
    math::decimal_division,
    msg::QueryValidators,
    state::{StakeType, CONFIG, CURRENT_BATCH, PARAMETERS, STATE},
};
use cosmwasm_std::{
    attr, to_json_binary, Coin, CosmosMsg, DepsMut, Env, MessageInfo, QueryRequest, Response,
    StakingMsg, StdError, StdResult, Uint128, ValidatorResponse, WasmMsg, WasmQuery,
};
use cw20::Cw20ExecuteMsg;

pub fn execute_stake(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    stake_type: StakeType,
) -> Result<Response, StdError> {
    let params = PARAMETERS.load(deps.storage)?;
    let staking_coin_denom = params.staking_coin_denom;

    let config = CONFIG.load(deps.storage)?;
    let sender = info.sender.clone();

    let reward_dispatcher_address = deps.api.addr_humanize(
        &config
            .reward_dispatcher_contract
            .ok_or_else(|| StdError::generic_err("Reward dispatcher contract is not set"))?,
    )?;

    //If stake type is StakeRewards, we need to check if the sender is the reward dispatcher contract
    if stake_type == StakeType::StakeRewards && sender != reward_dispatcher_address {
        return Err(StdError::generic_err(
            "Only the reward dispatcher contract can call this function",
        ));
    }

    let current_batch = CURRENT_BATCH.load(deps.storage)?;
    let requested_withdrawal_amount = current_batch.requested_lst_token_amount;

    if info.funds.len() > 1usize {
        return Err(StdError::generic_err(
            "Only one coin can be sent to the contract",
        ));
    }

    let payment = info
        .funds
        .iter()
        .find(|coin| coin.denom == staking_coin_denom && coin.amount > Uint128::zero())
        .ok_or_else(|| {
            StdError::generic_err(format!(
                "No {} assets are provided to bond",
                staking_coin_denom
            ))
        })?;

    // check slashing and get the latest exchange rate
    let state = check_slashing(&mut deps, env)?;

    let mut total_supply = query_total_lst_token_issued(deps.as_ref()).unwrap_or_default();

    let mint_amount = match stake_type {
        StakeType::LSTMint => decimal_division(payment.amount, state.lst_exchange_rate),
        StakeType::StakeRewards => Uint128::zero(),
    };

    total_supply += mint_amount;

    // state update
    STATE.update(deps.storage, |mut prev_state| -> StdResult<_> {
        match stake_type {
            StakeType::LSTMint => {
                todo!()
            }
            StakeType::StakeRewards => {
                todo!()
            }
        }
    })?;

    //validators management
    let validators_registry_contract =
        if let Some(validators_registry_contract) = config.validators_registry_contract {
            deps.api
                .addr_humanize(&validators_registry_contract)?
                .to_string()
        } else {
            return Err(StdError::generic_err(
                "Validators registry contract is not set",
            ));
        };

    let validators: Vec<ValidatorResponse> =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: validators_registry_contract,
            msg: to_json_binary(&QueryValidators::GetValidatorsForDelegation {})?,
        }))?;

    if validators.is_empty() {
        return Err(StdError::generic_err("No validators found"));
    }

    let (remaining_buffered_balance, delegations) =
        calculate_delegations(payment.amount, validators.as_slice());

    let mut external_call_msgs: Vec<cosmwasm_std::CosmosMsg> = vec![];
    for i in 0..delegations.len() {
        if delegations[i].is_zero() {
            continue;
        }

        external_call_msgs.push(CosmosMsg::Staking(StakingMsg::Delegate {
            validator: validators[i].validator.as_ref().unwrap().address.clone(),
            amount: Coin::new(delegations[i].u128(), payment.denom.as_str()),
        }));
    }

    //Skip minting of lst token in case of staking rewards
    if stake_type == StakeType::StakeRewards {
        let res = Response::new()
            .add_messages(external_call_msgs)
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

    let token_address = deps.api.addr_humanize(
        &config
            .lst_token
            .ok_or_else(|| StdError::generic_err("LST token address is not set"))?,
    )?;

    external_call_msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: token_address.to_string(),
        msg: to_json_binary(&mint_msg)?,
        funds: vec![],
    }));

    let res = Response::new()
        .add_messages(external_call_msgs)
        .add_attributes(vec![
            attr("action", "mint"),
            attr("from", sender.clone()),
            attr("staked", payment.amount),
            attr("minted", mint_amount),
        ]);

    Ok(res)
}
