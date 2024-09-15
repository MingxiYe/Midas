use std::{clone::Clone, fmt::Debug, time::Duration};

use bytes::Bytes;
use libafl::{
    corpus::{Corpus, Testcase},
    schedulers::Scheduler,
    state::{HasCorpus, HasMetadata},
};
use primitive_types::U512;
use revm_interpreter::Interpreter;
use revm_primitives::Bytecode;
use serde::{Deserialize, Serialize};

use crate::{
    evm::{
        host::FuzzHost,
        input::{ConciseEVMInput, EVMInput},
        types::{EVMAddress, EVMExecutionResult, EVMFuzzState, EVMU256},
        vm::EVMState,
    },
    input::ConciseSerde,
    txn_corpus::TxnResultMetadata,
};

#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize, Deserialize, Copy)]
pub enum MiddlewareType {
    OnChain,
    Concolic,
    Flashloan,
    // Selfdestruct,
    InstructionCoverage,
    BranchCoverage,
    Sha3Bypass,
    Sha3TaintAnalysis,
    CallPrinter,
    Reentrancy,
    IntegerOverflow,
    Cheatcode,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize, Deserialize, Copy)]
pub enum ExecutionStage {
    Call,
    Create,
    Log,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum CallMiddlewareReturn {
    Continue,
    ReturnRevert,
    ReturnSuccess(Bytes),
}

#[derive(Clone, Debug)]
pub enum MiddlewareOp {
    UpdateSlot(MiddlewareType, EVMAddress, EVMU256, EVMU256),
    UpdateCode(MiddlewareType, EVMAddress, Bytecode),
    AddCorpus(MiddlewareType, String, EVMAddress),
    AddCaller(MiddlewareType, EVMAddress),
    AddAddress(MiddlewareType, EVMAddress),
    AddBlacklist(MiddlewareType, EVMAddress),
    Owed(MiddlewareType, U512),
    Earned(MiddlewareType, U512),
    MakeSubsequentCallSuccess(Bytes),
}

pub fn add_corpus<SC>(host: &mut FuzzHost<SC>, state: &mut EVMFuzzState, input: &EVMInput)
where
    SC: Scheduler<State = EVMFuzzState> + Clone,
{
    let mut tc = Testcase::new(input.clone()) as Testcase<EVMInput>;
    tc.set_exec_time(Duration::from_secs(0));
    let idx = state.corpus_mut().add(tc.clone()).expect("failed to add");
    host.scheduler
        .on_add(state, idx)
        .expect("failed to call scheduler on_add");
    #[cfg(feature = "print_txn_corpus")]
    {
        let concise_input = ConciseEVMInput::from_input(input, &EVMExecutionResult::empty_result());
        let mut tc = state
            .corpus()
            .get(idx.clone())
            .expect("testcase not found")
            .borrow_mut();
        tc.add_metadata(TxnResultMetadata {
            is_revert: String::from("Unknown"),
            txn_text: concise_input.serialize_string(),
            txn_text_replayable: String::from_utf8(concise_input.serialize_concise()).unwrap(),
            idx: idx.to_string(),
            input_type: String::from("ABI Default"),
        });
        let _ = state.corpus().store_input_from(&tc);
    }
}

pub trait Middleware<SC>: Debug
where
    SC: Scheduler<State = EVMFuzzState> + Clone,
{
    #[allow(clippy::missing_safety_doc)]
    unsafe fn on_step(
        &mut self,
        interp: &mut Interpreter,
        host: &mut FuzzHost<SC>,
        state: &mut EVMFuzzState,
    );

    #[allow(clippy::missing_safety_doc)]
    unsafe fn on_return(
        &mut self,
        _interp: &mut Interpreter,
        _host: &mut FuzzHost<SC>,
        _state: &mut EVMFuzzState,
        _ret: &Bytes,
    ) {
    }

    #[allow(clippy::missing_safety_doc)]
    unsafe fn before_execute(
        &mut self,
        _interp: Option<&mut Interpreter>,
        _host: &mut FuzzHost<SC>,
        _state: &mut EVMFuzzState,
        _is_step: bool,
        _data: &mut Bytes,
        _evm_state: &mut EVMState,
    ) {
    }

    #[allow(clippy::missing_safety_doc)]
    unsafe fn on_insert(
        &mut self,
        _interp: Option<&mut Interpreter>,
        _host: &mut FuzzHost<SC>,
        _state: &mut EVMFuzzState,
        _bytecode: &mut Bytecode,
        _address: EVMAddress,
    ) {
    }
    fn get_type(&self) -> MiddlewareType;
}
