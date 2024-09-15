use crate::evm::input::{ConciseEVMInput, EVMInput};
use crate::evm::types::{EVMAddress, EVMFuzzState, EVMU256};
use crate::evm::vm::EVMState;
use crate::generic_vm::vm_executor::GenericVM;
use crate::generic_vm::vm_state::VMStateT;
use crate::input::{ConciseSerde, VMInputT};
use crate::oracle::{OracleCtx, Producer};
use crate::state::{FuzzState, HasExecutionResult};
use bytes::Bytes;
use revm_primitives::Bytecode;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt::Debug;

pub struct IcyProducer {
    // (caller, token) -> (init_balance, post_balance)
    pub balances: HashMap<(EVMAddress, EVMAddress), (EVMU256, EVMU256, EVMU256, EVMU256)>,
    pub balance_of: Vec<u8>,
}

impl IcyProducer {
    pub fn new() -> Self {
        Self {
            balances: HashMap::new(),
            balance_of: hex::decode("70a08231").unwrap(),
        }
    }
}

impl
    Producer<
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
    > for IcyProducer
{
    fn produce(
        &mut self,
        ctx: &mut OracleCtx<
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
        >,
    ) {
        #[cfg(feature = "flashloan_v2")]
        {
            let mut tokens = ctx
                .fuzz_state
                .get_execution_result()
                .new_state
                .state
                .flashloan_data
                .oracle_recheck_balance
                .clone();

            if let Some(post_state) = ctx.fuzz_state.post_state.clone() {
                for token in post_state.flashloan_data.oracle_recheck_balance.clone() {
                    tokens.insert(token);
                }
            }

            let callers = ctx.fuzz_state.callers_pool.clone();
            let query_balance_batch = callers
                .iter()
                .map(|caller| {
                    let mut extended_address = vec![0; 12];
                    extended_address.extend_from_slice(caller.0.as_slice());
                    let call_data =
                        Bytes::from([self.balance_of.clone(), extended_address].concat());
                    tokens
                        .iter()
                        .map(|token| (*token, call_data.clone()))
                        .collect::<Vec<(EVMAddress, Bytes)>>()
                })
                .flatten()
                .collect::<Vec<(EVMAddress, Bytes)>>();
            let post_balance_res = ctx.call_post_batch(&query_balance_batch);
            let pre_balance_res = ctx.call_pre_batch(&query_balance_batch);

            // calculate the balance in the target txn
            let txn = ctx.fuzz_state.txn.clone().expect("Target txn not found");
            let txn_post_balance_res = ctx.executor.borrow_mut().fast_static_call(
                &query_balance_batch,
                &ctx.fuzz_state
                    .post_state
                    .clone()
                    .expect("Post state not found"),
                ctx.fuzz_state,
            );
            let txn_pre_balance_res = ctx.executor.borrow_mut().fast_static_call(
                &query_balance_batch,
                txn.get_state(),
                ctx.fuzz_state,
            );

            let mut idx = 0;
            for caller in &callers {
                for token in &tokens {
                    let token = *token;
                    let pre_balance = &pre_balance_res[idx];
                    let post_balance = &post_balance_res[idx];
                    let txn_pre_balance = &txn_pre_balance_res[idx];
                    let txn_post_balance = &txn_post_balance_res[idx];
                    let old_balance =
                        EVMU256::try_from_be_slice(pre_balance.as_slice()).unwrap_or(EVMU256::ZERO);
                    let new_balance = EVMU256::try_from_be_slice(post_balance.as_slice())
                        .unwrap_or(EVMU256::ZERO);
                    let txn_old_balance = EVMU256::try_from_be_slice(&txn_pre_balance.as_slice())
                        .unwrap_or(EVMU256::ZERO);
                    let txn_new_balance = EVMU256::try_from_be_slice(&txn_post_balance.as_slice())
                        .unwrap_or(EVMU256::ZERO);

                    self.balances.insert(
                        (*caller, token),
                        (old_balance, new_balance, txn_old_balance, txn_new_balance),
                    );
                    idx += 1;
                }
            }
        }
    }

    fn notify_end(
        &mut self,
        ctx: &mut OracleCtx<
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
        >,
    ) {
        self.balances.clear();
    }
}
