use crate::evm::input::{ConciseEVMInput, EVMInput, EVMInputT};
use crate::evm::oracle::EVMBugResult;
use crate::evm::oracles::ICY_BUG_IDX;
use crate::evm::producers::icy_producer::IcyProducer;
use crate::evm::types::{EVMAddress, EVMFuzzState, EVMOracleCtx, EVMU256, EVMU512, EVMQueueExecutor};
#[cfg(feature = "flashloan_v2")]
use crate::evm::uniswap::{generate_uniswap_router_sell, TokenContext};
use crate::evm::vm::EVMState;
use crate::input::VMInputT;
use crate::oracle::Oracle;
use crate::state::HasExecutionResult;
use bytes::Bytes;
use revm_primitives::Bytecode;
use std::cell::RefCell;
#[cfg(feature = "flashloan_v2")]
use std::collections::HashMap;
#[cfg(feature = "flashloan_v2")]
use std::ops::Deref;
use std::rc::Rc;
use tracing::{debug, error};

pub struct IcyBugOracle {
    pub balance_of: Vec<u8>,
    #[cfg(feature = "flashloan_v2")]
    pub known_tokens: HashMap<EVMAddress, TokenContext>,
    #[cfg(feature = "flashloan_v2")]
    pub known_pair_reserve_slot: HashMap<EVMAddress, EVMU256>,
    #[cfg(feature = "flashloan_v2")]
    pub icy_producer: Rc<RefCell<IcyProducer>>,
}

impl IcyBugOracle {
    #[cfg(not(feature = "flashloan_v2"))]
    pub fn new(_: Rc<RefCell<PairProducer>>, _: Rc<RefCell<ERC20Producer>>) -> Self {
        Self {
            balance_of: hex::decode("70a08231").unwrap(),
        }
    }

    #[cfg(feature = "flashloan_v2")]
    pub fn new(icy_producer: Rc<RefCell<IcyProducer>>) -> Self {
        Self {
            balance_of: hex::decode("70a08231").unwrap(),
            known_tokens: HashMap::new(),
            known_pair_reserve_slot: HashMap::new(),
            icy_producer,
        }
    }

    #[cfg(feature = "flashloan_v2")]
    pub fn register_token(&mut self, token: EVMAddress, token_ctx: TokenContext) {
        self.known_tokens.insert(token, token_ctx);
    }

    #[cfg(feature = "flashloan_v2")]
    pub fn register_pair_reserve_slot(&mut self, pair: EVMAddress, slot: EVMU256) {
        self.known_pair_reserve_slot.insert(pair, slot);
    }
}

impl
    Oracle<
        EVMState,
        EVMAddress,
        Bytecode,
        Bytes,
        EVMAddress,
        EVMU256,
        Vec<u8>,
        EVMInput,
        EVMFuzzState,
        ConciseEVMInput,
    > for IcyBugOracle
{
    fn transition(&self, _ctx: &mut EVMOracleCtx<'_>, _stage: u64) -> u64 {
        0
    }

    #[cfg(not(feature = "flashloan_v2"))]
    fn oracle(&self, ctx: &mut EVMOracleCtx<'_>, _stage: u64) -> Vec<u64> {
        // has balance increased?
        let exec_res = &ctx.fuzz_state.get_execution_result().new_state.state;
        if exec_res.flashloan_data.earned > exec_res.flashloan_data.owed {
            EVMBugResult::new_simple(
                "icy".to_string(),
                ICY_BUG_IDX,
                format!(
                    "Earned {}wei more than owed {}wei",
                    exec_res.flashloan_data.earned, exec_res.flashloan_data.owed
                ),
                ConciseEVMInput::from_input(ctx.input, ctx.fuzz_state.get_execution_result()),
            )
            .push_to_output();
            vec![ICY_BUG_IDX]
        } else {
            vec![]
        }
    }

    #[cfg(feature = "flashloan_v2")]
    fn oracle(&self, ctx: &mut EVMOracleCtx<'_>, _stage: u64) -> Vec<u64> {
        let mut owed_token_not_found_flag = false;
        let mut earning_flag = ctx
            .fuzz_state
            .get_execution_result()
            .new_state
            .state
            .flashloan_data
            .earned
            + ctx
                .fuzz_state
                .post_state
                .clone()
                .unwrap()
                .flashloan_data
                .owed
            >= ctx
                .fuzz_state
                .post_state
                .clone()
                .unwrap()
                .flashloan_data
                .earned
                + ctx
                    .fuzz_state
                    .get_execution_result()
                    .new_state
                    .state
                    .flashloan_data
                    .owed;
        let mut real_earning_flag = ctx.fuzz_state.get_execution_result().new_state.state.flashloan_data.earned >= ctx.fuzz_state.get_execution_result().new_state.state.flashloan_data.owed;
        let mut zero_earning_flag = ctx
            .fuzz_state
            .get_execution_result()
            .new_state
            .state
            .flashloan_data
            .earned
            == ctx
                .fuzz_state
                .get_execution_result()
                .new_state
                .state
                .flashloan_data
                .owed;
        let mut similar_txn_flag = ctx
            .fuzz_state
            .get_execution_result()
            .new_state
            .state
            .flashloan_data
            .earned
            + ctx
                .fuzz_state
                .post_state
                .clone()
                .unwrap()
                .flashloan_data
                .owed
            == ctx
                .fuzz_state
                .post_state
                .clone()
                .unwrap()
                .flashloan_data
                .earned
                + ctx
                    .fuzz_state
                    .get_execution_result()
                    .new_state
                    .state
                    .flashloan_data
                    .owed;

        let mut liquidations_earned = Vec::new();
        let mut liquidations_owed = Vec::new();
        let mut txn_liquidations_earned = Vec::new();
        let mut txn_liquidations_owed = Vec::new();

        for ((caller, token), (old_balance, new_balance, txn_old_balance, txn_new_balance)) in
            self.icy_producer.deref().borrow().balances.iter()
        {
            earning_flag = earning_flag && new_balance + txn_old_balance >= txn_new_balance + old_balance;
            real_earning_flag = real_earning_flag && new_balance >= old_balance;
            zero_earning_flag = zero_earning_flag && new_balance == old_balance;
            similar_txn_flag = similar_txn_flag && new_balance + txn_old_balance == txn_new_balance + old_balance;

            if self.known_tokens.get(token).is_none() {
                error!("find unknown token: {}, old balance: {}, new balance: {}", token, old_balance, new_balance);
                continue; //todo@a3yip6
            }
            let token_info = self.known_tokens.get(token).expect("Token not found");

            if *new_balance > *old_balance {
                liquidations_earned.push((*caller, token_info, *new_balance - *old_balance));
            } else if *new_balance < *old_balance {
                liquidations_owed.push((*caller, token_info, *old_balance - *new_balance));
            }

            if *txn_new_balance > *txn_old_balance {
                txn_liquidations_earned.push((
                    *caller,
                    token_info,
                    *txn_new_balance - *txn_old_balance,
                ));
            } else if *txn_new_balance < *txn_old_balance {
                txn_liquidations_owed.push((
                    *caller,
                    token_info,
                    *txn_old_balance - *txn_new_balance,
                ));
            }
        }

        let path_idx = ctx.input.get_randomness()[0] as usize;

        let mut liquidation_txs_earned = vec![];
        let mut liquidation_txs_owed = vec![];
        let mut txn_liquidation_txs_earned = vec![];
        let mut txn_liquidation_txs_owed = vec![];
        for (caller, token_info, amount) in liquidations_earned {
            let txs = generate_uniswap_router_sell(
                token_info,
                path_idx,
                amount,
                ctx.fuzz_state.callers_pool[0],
            );
            if txs.is_none() {
                continue;
            }
            liquidation_txs_earned.extend(
                txs.unwrap()
                    .iter()
                    .map(|(abi, _, addr)| (caller, *addr, Bytes::from(abi.get_bytes()))),
            );
        }
        for (caller, token_info, amount) in liquidations_owed {
            let txs = generate_uniswap_router_sell(
                token_info,
                path_idx,
                amount,
                ctx.fuzz_state.callers_pool[0],
            );
            if txs.is_none() {
                owed_token_not_found_flag = true;
                continue;
            }
            liquidation_txs_owed.extend(
                txs.unwrap()
                    .iter()
                    .map(|(abi, _, addr)| (caller, *addr, Bytes::from(abi.get_bytes()))),
            );
        }
        for (caller, token_info, amount) in txn_liquidations_earned {
            let txs = generate_uniswap_router_sell(
                token_info,
                path_idx,
                amount,
                ctx.fuzz_state.callers_pool[0],
            );
            if txs.is_none() {
                continue;
            }
            txn_liquidation_txs_earned.extend(
                txs.unwrap()
                    .iter()
                    .map(|(abi, _, addr)| (caller, *addr, Bytes::from(abi.get_bytes()))),
            );
        }
        for (caller, token_info, amount) in txn_liquidations_owed {
            let txs = generate_uniswap_router_sell(
                token_info,
                path_idx,
                amount,
                ctx.fuzz_state.callers_pool[0],
            );
            if txs.is_none() {
                continue;
            }
            txn_liquidation_txs_owed.extend(
                txs.unwrap()
                    .iter()
                    .map(|(abi, _, addr)| (caller, *addr, Bytes::from(abi.get_bytes()))),
            );
        }

        let (_out, mut this_state_earned) = ctx.call_post_batch_dyn(&liquidation_txs_earned);
        let (_out, this_state_owed) = ctx.call_pre_batch_dyn(&liquidation_txs_owed);

        let (_out, mut txn_state_earned) = ctx.executor.deref().borrow_mut().fast_call(
            &txn_liquidation_txs_earned,
            &ctx.fuzz_state
                .post_state
                .clone()
                .expect("Post state not found"),
            ctx.fuzz_state,
        );
        let (_out, txn_state_owed) = ctx.executor.deref().borrow_mut().fast_call(
            &txn_liquidation_txs_owed,
            ctx.fuzz_state.txn.clone().unwrap().get_state(),
            ctx.fuzz_state,
        );

        this_state_earned.flashloan_data.owed += this_state_owed.flashloan_data.earned;
        txn_state_earned.flashloan_data.owed += txn_state_owed.flashloan_data.earned;

        let exec_res = ctx.fuzz_state.get_execution_result_mut();
        exec_res
            .new_state
            .state
            .flashloan_data
            .oracle_recheck_balance
            .clear();
        exec_res
            .new_state
            .state
            .flashloan_data
            .oracle_recheck_reserve
            .clear();

        if owed_token_not_found_flag {
            exec_res.new_state.state.flashloan_data.owed = ctx.input.get_state().flashloan_data.owed;
            exec_res.new_state.state.flashloan_data.earned = ctx.input.get_state().flashloan_data.earned;
        } else {
            exec_res.new_state.state.flashloan_data.owed = this_state_earned.flashloan_data.owed;
            exec_res.new_state.state.flashloan_data.earned = this_state_earned.flashloan_data.earned;
        }

        earning_flag = earning_flag
            && this_state_earned.flashloan_data.earned != EVMU512::from(0)
            && (txn_state_earned.flashloan_data.earned != EVMU512::from(0) || real_earning_flag)
            && this_state_earned.flashloan_data.earned + txn_state_earned.flashloan_data.owed
                >= txn_state_earned.flashloan_data.earned
                    + this_state_earned.flashloan_data.owed
                    + EVMU512::from(10_000_000_000_000_000_000_u128);
        zero_earning_flag = zero_earning_flag
            || this_state_earned.flashloan_data.earned == this_state_earned.flashloan_data.owed;
        similar_txn_flag = similar_txn_flag
            || (this_state_earned.flashloan_data.earned == txn_state_earned.flashloan_data.earned
                && this_state_earned.flashloan_data.owed == txn_state_earned.flashloan_data.owed);

        if earning_flag && !zero_earning_flag && !similar_txn_flag {
            let net = this_state_earned.flashloan_data.earned
                + txn_state_earned.flashloan_data.owed
                - txn_state_earned.flashloan_data.earned
                - this_state_earned.flashloan_data.owed;
            // we scaled by 1e24, so divide by 1e24 to get ETH
            let net_eth = net / EVMU512::from(1_000_000_000_000_000_000_000_u128);
            EVMBugResult::new_simple(
                "icy_bug".to_string(),
                ICY_BUG_IDX,
                format!(
                    "💰[IcyBugOracle] The generated Path : Earned {} more than owed {}\n💰[IcyBugOracle] The original Path: Earned {} more than owed {},\n Total earned {}, extra: {:?}\n",
                    this_state_earned.flashloan_data.earned,
                    this_state_earned.flashloan_data.owed,
                    EVMU512::from(txn_state_earned.flashloan_data.earned),
                    EVMU512::from(txn_state_earned.flashloan_data.owed),
                    net_eth,
                    this_state_earned.flashloan_data.extra_info
                ),
                ConciseEVMInput::from_input(ctx.input, ctx.fuzz_state.get_execution_result()),
            )
            .push_to_output();
            vec![ICY_BUG_IDX]
        } else {
            vec![]
        }
    }
}
