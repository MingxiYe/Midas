use crate::evm::input::{ConciseEVMInput, EVMInputT};
use crate::evm::middlewares::sha3_bypass::Sha3TaintAnalysis;
use crate::evm::types::EVMAddress;
use crate::evm::vm::EVMExecutor;
use crate::generic_vm::vm_state::VMStateT;
use crate::input::VMInputT;
use crate::state::{HasCaller, HasCurrentInputIdx, HasExecutionResult, HasItyState};
use libafl::events::EventFirer;
use libafl::executors::ExitKind;
use libafl::feedbacks::Feedback;
use libafl::observers::ObserversTuple;
use libafl::prelude::{HasCorpus, HasMetadata, HasRand, Input, State, Testcase, UsesInput};
use libafl::schedulers::Scheduler;
use libafl::state::HasClientPerfMonitor;
use libafl::Error;
use libafl_bolts::Named;
use std::cell::RefCell;
use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;
use std::ops::Deref;
use std::rc::Rc;

use super::vm::EVMState;

/// A wrapper around a feedback that also performs sha3 taint analysis
/// when the feedback is interesting.
pub struct Sha3WrappedFeedback<I, S, VS, F, SC>
where
    S: State + HasCorpus + HasCaller<EVMAddress> + Debug + Clone + HasClientPerfMonitor + 'static,
    I: VMInputT<VS, EVMAddress, EVMAddress, ConciseEVMInput> + EVMInputT,
    VS: VMStateT,
    F: Feedback<S>,
    SC: Scheduler<State = S> + Clone,
{
    pub inner_feedback: Box<F>,
    pub sha3_taints: Rc<RefCell<Sha3TaintAnalysis>>,
    pub evm_executor: Rc<RefCell<EVMExecutor<I, S, VS, ConciseEVMInput, SC>>>,
    pub enabled: bool,
}

impl<I, S, VS, F, SC> Feedback<S> for Sha3WrappedFeedback<I, S, VS, F, SC>
where
    S: State
        + HasRand
        + HasCorpus
        + HasItyState<EVMAddress, EVMAddress, VS, ConciseEVMInput>
        + HasMetadata
        + HasCaller<EVMAddress>
        + HasCurrentInputIdx
        + HasClientPerfMonitor
        + Default
        + Clone
        + Debug
        + UsesInput<Input = I>
        + 'static,
    I: VMInputT<VS, EVMAddress, EVMAddress, ConciseEVMInput> + EVMInputT + 'static,
    VS: VMStateT + 'static,
    F: Feedback<S>,
    SC: Scheduler<State = S> + Clone + 'static,
{
    fn is_interesting<EM, OT>(
        &mut self,
        state: &mut S,
        manager: &mut EM,
        input: &S::Input,
        observers: &OT,
        exit_kind: &ExitKind,
    ) -> Result<bool, Error>
    where
        EM: EventFirer<State = S>,
        OT: ObserversTuple<S>,
    {
        // checks if the inner feedback is interesting
        if self.enabled {
            match self
                .inner_feedback
                .is_interesting(state, manager, input, observers, exit_kind)
            {
                Ok(true) => {
                    if !input.is_step() {
                        // reexecute with sha3 taint analysis
                        self.sha3_taints.deref().borrow_mut().cleanup();

                        (self.evm_executor.deref().borrow_mut()).reexecute_with_middleware(
                            input,
                            state,
                            self.sha3_taints.clone(),
                        );
                    }
                    Ok(true)
                }
                Ok(false) => Ok(false),
                Err(e) => Err(e),
            }
        } else {
            self.inner_feedback
                .is_interesting(state, manager, input, observers, exit_kind)
        }
    }

    #[inline]
    #[allow(unused_variables)]
    fn append_metadata<OT>(
        &mut self,
        state: &mut S,
        observers: &OT,
        testcase: &mut Testcase<S::Input>,
    ) -> Result<(), Error>
    where
        OT: ObserversTuple<S>,
    {
        self.inner_feedback
            .as_mut()
            .append_metadata(state, observers, testcase)
    }
}

impl<I, S, VS, F, SC> Sha3WrappedFeedback<I, S, VS, F, SC>
where
    S: State + HasCorpus + HasCaller<EVMAddress> + Debug + Clone + HasClientPerfMonitor + 'static,
    I: VMInputT<VS, EVMAddress, EVMAddress, ConciseEVMInput> + EVMInputT,
    VS: VMStateT,
    F: Feedback<S>,
    SC: Scheduler<State = S> + Clone,
{
    pub(crate) fn new(
        inner_feedback: F,
        sha3_taints: Rc<RefCell<Sha3TaintAnalysis>>,
        evm_executor: Rc<RefCell<EVMExecutor<I, S, VS, ConciseEVMInput, SC>>>,
        enabled: bool,
    ) -> Self {
        Self {
            inner_feedback: Box::new(inner_feedback),
            sha3_taints,
            evm_executor,
            enabled,
        }
    }
}

impl<I, S, VS, F, SC> Named for Sha3WrappedFeedback<I, S, VS, F, SC>
where
    S: State + HasCorpus + HasCaller<EVMAddress> + Debug + Clone + HasClientPerfMonitor + 'static,
    I: VMInputT<VS, EVMAddress, EVMAddress, ConciseEVMInput> + EVMInputT,
    VS: VMStateT,
    F: Feedback<S>,
    SC: Scheduler<State = S> + Clone,
{
    fn name(&self) -> &str {
        todo!()
    }
}

impl<I, S, VS, F, SC> Debug for Sha3WrappedFeedback<I, S, VS, F, SC>
where
    S: State + HasCorpus + HasCaller<EVMAddress> + Debug + Clone + HasClientPerfMonitor + 'static,
    I: VMInputT<VS, EVMAddress, EVMAddress, ConciseEVMInput> + EVMInputT,
    VS: VMStateT,
    F: Feedback<S>,
    SC: Scheduler<State = S> + Clone,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        todo!()
    }
}

/// A wrapper around a path coverage feedback and a branch coverage feedback
/// The input is interesting, if the coverage increase without revert
/// or JUMPI opcode coverage increase
pub struct JmpWrappedFeedback<I, S, VS, F, SC>
where
    S: State + HasCorpus + HasCaller<EVMAddress> + Debug + Clone + HasClientPerfMonitor + 'static,
    I: VMInputT<VS, EVMAddress, EVMAddress, ConciseEVMInput> + EVMInputT,
    VS: VMStateT,
    F: Feedback<S>,
    SC: Scheduler<State = S> + Clone,
{
    pub cov_feedback: Box<F>,
    pub jmp_op_feedback: Box<F>,
    pub transfer_diff_feedback: Box<F>,
    pub sha3_taints: Rc<RefCell<Sha3TaintAnalysis>>,
    pub evm_executor: Rc<RefCell<EVMExecutor<I, S, VS, ConciseEVMInput, SC>>>,
    pub enabled: bool,
}

impl<I, S, VS, F, SC> JmpWrappedFeedback<I, S, VS, F, SC>
where
    S: State + HasCorpus + HasCaller<EVMAddress> + Debug + Clone + HasClientPerfMonitor + 'static,
    I: VMInputT<VS, EVMAddress, EVMAddress, ConciseEVMInput> + EVMInputT,
    VS: VMStateT,
    F: Feedback<S>,
    SC: Scheduler<State = S> + Clone,
{
    pub(crate) fn new(
        cov_feedback: F,
        jmp_op_feedback: F,
        transfer_diff_feedback: F,
        sha3_taints: Rc<RefCell<Sha3TaintAnalysis>>,
        evm_executor: Rc<RefCell<EVMExecutor<I, S, VS, ConciseEVMInput, SC>>>,
        enabled: bool,
    ) -> Self {
        Self {
            cov_feedback: Box::new(cov_feedback),
            jmp_op_feedback: Box::new(jmp_op_feedback),
            transfer_diff_feedback: Box::new(transfer_diff_feedback),
            sha3_taints,
            evm_executor,
            enabled,
        }
    }
}

impl<I, S, VS, F, SC> Named for JmpWrappedFeedback<I, S, VS, F, SC>
where
    S: State + HasCorpus + HasCaller<EVMAddress> + Debug + Clone + HasClientPerfMonitor + 'static,
    I: VMInputT<VS, EVMAddress, EVMAddress, ConciseEVMInput> + EVMInputT,
    VS: VMStateT,
    F: Feedback<S>,
    SC: Scheduler<State = S> + Clone,
{
    fn name(&self) -> &str {
        "JmpWrappedFeedback"
    }
}

impl<I, S, VS, F, SC> Debug for JmpWrappedFeedback<I, S, VS, F, SC>
where
    S: State + HasCorpus + HasCaller<EVMAddress> + Debug + Clone + HasClientPerfMonitor + 'static,
    I: VMInputT<VS, EVMAddress, EVMAddress, ConciseEVMInput> + EVMInputT,
    VS: VMStateT,
    F: Feedback<S>,
    SC: Scheduler<State = S> + Clone,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        f.debug_struct("JmpWrappedFeedback").finish()
    }
}

impl<I, S, VS, F, SC> Feedback<S> for JmpWrappedFeedback<I, S, VS, F, SC>
where
    S: State
        + HasRand
        + HasCorpus
        + HasItyState<EVMAddress, EVMAddress, VS, ConciseEVMInput>
        + HasMetadata
        + HasCaller<EVMAddress>
        + HasCurrentInputIdx
        + HasClientPerfMonitor
        + HasExecutionResult<EVMAddress, EVMAddress, VS, Vec<u8>, ConciseEVMInput>
        + Default
        + Clone
        + Debug
        + UsesInput<Input = I>
        + 'static,
    I: VMInputT<VS, EVMAddress, EVMAddress, ConciseEVMInput> + EVMInputT + 'static,
    VS: VMStateT + 'static,
    F: Feedback<S>,
    SC: Scheduler<State = S> + Clone + 'static,
{
    fn is_interesting<EM, OT>(
        &mut self,
        state: &mut S,
        manager: &mut EM,
        input: &S::Input,
        observers: &OT,
        exit_kind: &ExitKind,
    ) -> Result<bool, Error>
    where
        EM: EventFirer<State = S>,
        OT: ObserversTuple<S>,
    {
        // checks if the inner feedback is interesting
        let result = if self
            .cov_feedback
            .is_interesting(state, manager, input, observers, exit_kind)?
            && !state.get_execution_result().reverted
        {
            Ok(true)
        } else if self
            .jmp_op_feedback
            .is_interesting(state, manager, input, observers, exit_kind)?
        {
            Ok(true)
        } else if self
            .transfer_diff_feedback
            .is_interesting(state, manager, input, observers, exit_kind)?
        {
            Ok(true)
        } else {
            Ok(false)
        };
        if self.enabled {
            match result {
                Ok(true) => {
                    if !input.is_step() {
                        // reexecute with sha3 taint analysis
                        self.sha3_taints.deref().borrow_mut().cleanup();

                        (self.evm_executor.deref().borrow_mut()).reexecute_with_middleware(
                            input,
                            state,
                            self.sha3_taints.clone(),
                        );
                    }
                    Ok(true)
                }
                Ok(false) => Ok(false),
                Err(e) => Err(e),
            }
        } else {
            result
        }
    }

    #[inline]
    #[allow(unused_variables)]
    fn append_metadata<OT>(
        &mut self,
        state: &mut S,
        observers: &OT,
        testcase: &mut Testcase<S::Input>,
    ) -> Result<(), Error>
    where
        OT: ObserversTuple<S>,
    {
        self.jmp_op_feedback
            .as_mut()
            .append_metadata(state, observers, testcase);
        self.transfer_diff_feedback
            .as_mut()
            .append_metadata(state, observers, testcase);
        self.cov_feedback
            .as_mut()
            .append_metadata(state, observers, testcase)
    }
}