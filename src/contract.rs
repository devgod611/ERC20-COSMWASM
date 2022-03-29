use cosmwasm_std::{
    entry_point, to_binary, to_vec, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Response,
    StdResult, Storage, Uint128, WasmMsg, CosmosMsg, from_slice
};
use cosmwasm_storage::{PrefixedStorage, ReadonlyPrefixedStorage};
use std::convert::TryInto;
use cw20::{Cw20ExecuteMsg};

use crate::error::ContractError;
use crate::msg::{AllowanceResponse, BalanceResponse, ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{Constants, startTime, endTime, communityFundRewardRate, devFundRewardRate,
    communityFund, devFund, communityFundLastClaimed, devFundLastClaimed, rewardPoolDistributed,
};

pub const PREFIX_CONFIG: &[u8] = b"config";
pub const PREFIX_BALANCES: &[u8] = b"balances";
pub const PREFIX_ALLOWANCES: &[u8] = b"allowances";

pub const KEY_CONSTANTS: &[u8] = b"constants";
pub const KEY_TOTAL_SUPPLY: &[u8] = b"total_supply";

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> 
{
    let mut config_store = PrefixedStorage::new(deps.storage, PREFIX_CONFIG);

    let ether: Uint128 = Uint128::from((10 as u128).pow(18 as u32));
    let day: Uint128 = Uint128::from((60 * 60 * 24) as u128);

    let FARMING_POOL_REWARD_ALLOCATION: Uint128 = Uint128::from(60000 as u128) * ether;
    let COMMUNITY_FUND_POOL_ALLOCATION: Uint128 = Uint128::zero();

    let DEV_FUND_POOL_ALLOCATION: Uint128 = Uint128::from(5000 as u128) * ether;
    let VESTING_DURATION: Uint128 = Uint128::from(356 as u128) * day;

    let constants = to_vec(&Constants {
        name: "3SHARE Token".to_string(),
        symbol: "3SHARES".to_string(),
        decimals: 18,
        ether,
        day,
        FARMING_POOL_REWARD_ALLOCATION,
        COMMUNITY_FUND_POOL_ALLOCATION,
        DEV_FUND_POOL_ALLOCATION,
        VESTING_DURATION
    })?;
    config_store.set(KEY_CONSTANTS, &constants);

    let total_supply: u128 = 0;
    config_store.set(KEY_TOTAL_SUPPLY, &total_supply.to_be_bytes());

    let amount = ether;
    perform_mint(deps.storage, _env,  _info.sender, amount)?;

    rewardPoolDistributed.save(deps.storage, &(false))?;

    let start_time = msg._startTime;
    let end_time = msg._startTime + Uint128::from(365 as u32) * day;

    startTime.save(deps.storage, &(start_time))?;
    endTime.save(deps.storage, &(end_time))?;

    communityFundLastClaimed.save(deps.storage, &(start_time))?;
    devFundLastClaimed.save(deps.storage, &(start_time))?;

    let _cRate = COMMUNITY_FUND_POOL_ALLOCATION / VESTING_DURATION;
    communityFundRewardRate.save(deps.storage, &_cRate)?;

    let _dRate = DEV_FUND_POOL_ALLOCATION /VESTING_DURATION;
    devFundRewardRate.save(deps.storage, &_dRate)?;

    let _devFund = deps.api.addr_validate(msg._devFund.as_str())?;
    let _communityFund = deps.api.addr_validate(msg._communityFund.as_str())?;
    devFund.save(deps.storage, &_devFund)?;
    communityFund.save(deps.storage, &_communityFund)?;

    Ok(Response::default())
}

#[entry_point]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Approve { spender, amount } => try_approve(deps, env, info, spender, &amount),
        ExecuteMsg::Transfer { recipient, amount } => {
            try_transfer(deps, env, info, recipient, &amount)
        }
        ExecuteMsg::TransferFrom {
            owner,
            recipient,
            amount,
        } => try_transfer_from(deps, env, info, owner, recipient, &amount),
        ExecuteMsg::Burn { amount } => try_burn(deps, env, info, &amount),
        ExecuteMsg::Mint { recipient, amount } => try_mint(deps, env, info, recipient, amount),

        ExecuteMsg::setTreasuryFund { _communityFund } =>
            try_settreasureyfund(deps, env, info, _communityFund),
        ExecuteMsg::setDevFund { _devFund } =>
            try_setdevfund(deps, env, info, _devFund),
        ExecuteMsg::claimRewards { } =>
            try_claimrewards(deps, env, info),
        ExecuteMsg::distributeReward { _farmingIncentiveFund } => 
            try_distributereward(deps, env, info, _farmingIncentiveFund),
        ExecuteMsg::governanceRecoverUnsupported{ _token, _amount, _to} =>
            try_governancerecoverunsupported(deps, env, info, _token, _amount, _to)
    }
}

fn try_governancerecoverunsupported(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    _token: Addr,
    _amount: Uint128,
    _to: Addr,
) -> Result<Response, ContractError> 
{
    let bank_cw20 = WasmMsg::Execute {
        contract_addr: String::from(_token),
        msg: to_binary(&Cw20ExecuteMsg::Transfer {
            recipient: _to.to_string(),
            amount: _amount,
        }).unwrap(),
        funds: Vec::new()
    };

    Ok(Response::new()
    .add_message(CosmosMsg::Wasm(bank_cw20))
    .add_attribute("action", "governanceRecoverUnsupported"))
}
fn try_distributereward(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    _farmingIncentiveFund: Addr,
) -> Result<Response, ContractError> 
{
    let _devFund = devFund.load(deps.storage)?;
    if info.sender != _devFund {
        return Err(ContractError::NotOperator{});
    }
    let mut _rewardPoolDistributed = rewardPoolDistributed.load(deps.storage)?;
    if _rewardPoolDistributed {
        return Err(ContractError::DoubleDistrubute{})
    }
    _rewardPoolDistributed = true;
    rewardPoolDistributed.save(deps.storage, &_rewardPoolDistributed);

    let config_storage = ReadonlyPrefixedStorage::new(deps.storage, PREFIX_CONFIG);
    let data = config_storage
            .get(KEY_CONSTANTS)
            .expect("no config data stored");
    let constants:Constants = from_slice(&data).expect("invalid data");

    try_mint(deps, _env, info,_farmingIncentiveFund, constants.FARMING_POOL_REWARD_ALLOCATION);

    Ok(Response::new()
    .add_attribute("action", "distributeReward"))
}

fn try_claimrewards(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> 
{
    let mut _pending: Uint128 = _unclaimedTreasuryFund(deps.storage, _env.clone());
    if _pending > Uint128::zero() {
        let _communityFund = communityFund.load(deps.storage)?;
        perform_mint(deps.storage, _env.clone(),  _communityFund, _pending);
        let _now = Uint128::from(_env.clone().block.time.seconds());
        communityFundLastClaimed.save(deps.storage, &_now);
    }

    _pending = _unclaimedDevFund(deps.storage, _env.clone());
    if _pending > Uint128::zero() {
        let _devFund = devFund.load(deps.storage)?;
        perform_mint(deps.storage, _env.clone(),  _devFund, _pending);
        let _now = Uint128::from(_env.block.time.seconds());
        devFundLastClaimed.save(deps.storage, &_now);
    }

    Ok(Response::new()
        .add_attribute("action", "claimRewards"))
}
fn try_settreasureyfund(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    _communityFund: Addr,
) -> Result<Response, ContractError> 
{
    let _devFund = devFund.load(deps.storage)?;
    if info.sender != _devFund {
        return Err(ContractError::NotOperator{});
    }
    communityFund.save(deps.storage, &_communityFund);

    Ok(Response::new()
        .add_attribute("action", "setTreasuryFund")
        .add_attribute("communityFund", _communityFund.to_string()))
}
fn try_setdevfund(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    _devFund: Addr,
) -> Result<Response, ContractError> 
{
    let _devFund = devFund.load(deps.storage)?;
    if info.sender != _devFund {
        return Err(ContractError::NotOperator{});
    }
    devFund.save(deps.storage, &_devFund);

    Ok(Response::new()
        .add_attribute("action", "setTreasuryFund")
        .add_attribute("devFund", _devFund.to_string()))
}
fn try_transfer(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    recipient: String,
    amount: &Uint128,
) -> Result<Response, ContractError> {
    perform_transfer(
        deps.storage,
        &info.sender,
        &deps.api.addr_validate(recipient.as_str())?,
        amount.u128(),
    )?;
    Ok(Response::new()
        .add_attribute("action", "transfer")
        .add_attribute("sender", info.sender)
        .add_attribute("recipient", recipient))
}

fn try_transfer_from(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    owner: String,
    recipient: String,
    amount: &Uint128,
) -> Result<Response, ContractError> {
    let owner_address = deps.api.addr_validate(owner.as_str())?;
    let recipient_address = deps.api.addr_validate(recipient.as_str())?;
    let amount_raw = amount.u128();

    let mut allowance = read_allowance(deps.storage, &owner_address, &info.sender)?;
    if allowance < amount_raw {
        return Err(ContractError::InsufficientAllowance {
            allowance,
            required: amount_raw,
        });
    }
    allowance -= amount_raw;
    write_allowance(deps.storage, &owner_address, &info.sender, allowance)?;
    perform_transfer(deps.storage, &owner_address, &recipient_address, amount_raw)?;

    Ok(Response::new()
        .add_attribute("action", "transfer_from")
        .add_attribute("spender", &info.sender)
        .add_attribute("sender", owner)
        .add_attribute("recipient", recipient))
}

fn try_approve(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    spender: String,
    amount: &Uint128,
) -> Result<Response, ContractError> {
    let spender_address = deps.api.addr_validate(spender.as_str())?;
    write_allowance(deps.storage, &info.sender, &spender_address, amount.u128())?;
    Ok(Response::new()
        .add_attribute("action", "approve")
        .add_attribute("owner", info.sender)
        .add_attribute("spender", spender))
}


fn try_mint(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    recipient: Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {
    perform_mint(deps.storage, _env, recipient.clone(), amount);
    Ok(Response::new()
        .add_attribute("action", "mint")
        .add_attribute("recipient", recipient.to_string())
        .add_attribute("amount", amount.to_string()))
}
fn perform_mint(
    store: &mut dyn Storage,
    _env: Env,
    recipient: Addr,
    amount: Uint128
) -> Result<(), ContractError> {
    let mut balances_store = PrefixedStorage::new(store, PREFIX_BALANCES);

    let mut from_balance = match balances_store.get(recipient.as_str().as_bytes()) {
        Some(data) => bytes_to_u128(&data),
        None => Ok(0u128),
    }?;

    from_balance += amount.u128();
    balances_store.set(recipient.as_str().as_bytes(), &from_balance.to_be_bytes());

    let mut config_store = PrefixedStorage::new(store, PREFIX_CONFIG);
    let data = config_store
        .get(KEY_TOTAL_SUPPLY)
        .expect("no total supply data stored");
    let mut total_supply = bytes_to_u128(&data).unwrap();

    total_supply += amount.u128();

    config_store.set(KEY_TOTAL_SUPPLY, &total_supply.to_be_bytes());


    Ok(())
}

/// Burn tokens
///
/// Remove `amount` tokens from the system irreversibly, from signer account
///
/// @param amount the amount of money to burn
fn try_burn(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    amount: &Uint128,
) -> Result<Response, ContractError> {
    let amount_raw = amount.u128();

    let mut account_balance = read_balance(deps.storage, &info.sender)?;

    if account_balance < amount_raw {
        return Err(ContractError::InsufficientFunds {
            balance: account_balance,
            required: amount_raw,
        });
    }
    account_balance -= amount_raw;

    let mut balances_store = PrefixedStorage::new(deps.storage, PREFIX_BALANCES);
    balances_store.set(
        info.sender.as_str().as_bytes(),
        &account_balance.to_be_bytes(),
    );

    let mut config_store = PrefixedStorage::new(deps.storage, PREFIX_CONFIG);
    let data = config_store
        .get(KEY_TOTAL_SUPPLY)
        .expect("no total supply data stored");
    let mut total_supply = bytes_to_u128(&data).unwrap();

    total_supply -= amount_raw;

    config_store.set(KEY_TOTAL_SUPPLY, &total_supply.to_be_bytes());

    Ok(Response::new()
        .add_attribute("action", "burn")
        .add_attribute("account", info.sender)
        .add_attribute("amount", amount.to_string()))
}

fn perform_transfer(
    store: &mut dyn Storage,
    from: &Addr,
    to: &Addr,
    amount: u128,
) -> Result<(), ContractError> {
    let mut balances_store = PrefixedStorage::new(store, PREFIX_BALANCES);

    let mut from_balance = match balances_store.get(from.as_str().as_bytes()) {
        Some(data) => bytes_to_u128(&data),
        None => Ok(0u128),
    }?;

    if from_balance < amount {
        return Err(ContractError::InsufficientFunds {
            balance: from_balance,
            required: amount,
        });
    }
    from_balance -= amount;
    balances_store.set(from.as_str().as_bytes(), &from_balance.to_be_bytes());

    let mut to_balance = match balances_store.get(to.as_str().as_bytes()) {
        Some(data) => bytes_to_u128(&data),
        None => Ok(0u128),
    }?;
    to_balance += amount;
    balances_store.set(to.as_str().as_bytes(), &to_balance.to_be_bytes());

    Ok(())
}

// Converts 16 bytes value into u128
// Errors if data found that is not 16 bytes
pub fn bytes_to_u128(data: &[u8]) -> Result<u128, ContractError> {
    match data[0..16].try_into() {
        Ok(bytes) => Ok(u128::from_be_bytes(bytes)),
        Err(_) => Err(ContractError::CorruptedDataFound {}),
    }
}

// Reads 16 byte storage value into u128
// Returns zero if key does not exist. Errors if data found that is not 16 bytes
pub fn read_u128(store: &ReadonlyPrefixedStorage, key: &Addr) -> Result<u128, ContractError> {
    let result = store.get(key.as_str().as_bytes());
    match result {
        Some(data) => bytes_to_u128(&data),
        None => Ok(0u128),
    }
}
fn _unclaimedTreasuryFund(
    store: & dyn Storage,
    _env: Env,
) -> Uint128
{
    let mut _now = Uint128::from(_env.block.time.seconds());
    let end_time = endTime.load(store).unwrap();
    if _now > end_time {
        _now = end_time;
    }
    let _cLastClaimed = communityFundLastClaimed.load(store).unwrap();
    let mut out = Uint128::zero();
    if _cLastClaimed < _now {
        let _cRewardRate = communityFundRewardRate.load(store).unwrap();
        let _pending = (_now - _cLastClaimed) * _cRewardRate;
        out = _pending;
    }
    return out;
}
fn _unclaimedDevFund(
    store: & dyn Storage,
    _env: Env,
) -> Uint128
{
    let mut _now = Uint128::from(_env.block.time.seconds());
    let end_time = endTime.load(store).unwrap();
    if _now > end_time {
        _now = end_time;
    }
    let _dLastClaimed = devFundLastClaimed.load(store).unwrap();
    let mut out = Uint128::zero();
    if _dLastClaimed < _now {
        let _dRewardRate = devFundRewardRate.load(store).unwrap();
        let _pending = (_now - _dLastClaimed) * _dRewardRate;
        out = _pending;
    }
    return out;
}
#[entry_point]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::Balance { address } => {
            let address_key = deps.api.addr_validate(&address)?;
            let balance = read_balance(deps.storage, &address_key)?;
            let out = to_binary(&BalanceResponse {
                balance: Uint128::from(balance),
            })?;
            Ok(out)
        }
        QueryMsg::Allowance { owner, spender } => {
            let owner_key = deps.api.addr_validate(&owner)?;
            let spender_key = deps.api.addr_validate(&spender)?;
            let allowance = read_allowance(deps.storage, &owner_key, &spender_key)?;
            let out = to_binary(&AllowanceResponse {
                allowance: Uint128::from(allowance),
            })?;
            Ok(out)
        }
        QueryMsg::unclaimedTreasuryFund{ } => {
            let out = _unclaimedTreasuryFund(deps.storage, _env);
            Ok(to_binary(&out)?)
        }
        QueryMsg::unclaimedDevFund{ } => {
            let out = _unclaimedDevFund(deps.storage, _env);
            Ok(to_binary(&out)?)
        }        
    }
}

fn read_balance(store: &dyn Storage, owner: &Addr) -> Result<u128, ContractError> {
    let balance_store = ReadonlyPrefixedStorage::new(store, PREFIX_BALANCES);
    read_u128(&balance_store, owner)
}

fn read_allowance(
    store: &dyn Storage,
    owner: &Addr,
    spender: &Addr,
) -> Result<u128, ContractError> {
    let owner_store =
        ReadonlyPrefixedStorage::multilevel(store, &[PREFIX_ALLOWANCES, owner.as_str().as_bytes()]);
    read_u128(&owner_store, spender)
}

#[allow(clippy::unnecessary_wraps)]
fn write_allowance(
    store: &mut dyn Storage,
    owner: &Addr,
    spender: &Addr,
    amount: u128,
) -> StdResult<()> {
    let mut owner_store =
        PrefixedStorage::multilevel(store, &[PREFIX_ALLOWANCES, owner.as_str().as_bytes()]);
    owner_store.set(spender.as_str().as_bytes(), &amount.to_be_bytes());
    Ok(())
}

fn is_valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.len() < 3 || bytes.len() > 30 {
        return false;
    }
    true
}

fn is_valid_symbol(symbol: &str) -> bool {
    let bytes = symbol.as_bytes();
    if bytes.len() < 3 || bytes.len() > 6 {
        return false;
    }
    for byte in bytes.iter() {
        if *byte < 65 || *byte > 90 {
            return false;
        }
    }
    true
}
