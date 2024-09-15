/// A corpus for transactions with custom storage strategies.
/// This is a wrapped inmemory corpus.
use core::cell::RefCell;
use libafl_bolts::impl_serdeany;
use serde::{Deserialize, Serialize};
use std::{
    cell::{Ref, RefMut},
    fs::{File, OpenOptions},
    io::Write,
    path::Path,
};
use tracing::info;

use libafl::{
    corpus::{Corpus, InMemoryCorpus, Testcase},
    inputs::{Input, UsesInput},
    prelude::{CorpusId, HasTestcase},
    state::{HasMetadata, State},
    Error,
};

use crate::fuzzer::REPLAY;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct TxnResultMetadata {
    pub is_revert: String,
    pub txn_text: String,
    pub txn_text_replayable: String,
    pub idx: String,
    pub input_type: String,
}

impl TxnResultMetadata {
    /// Create a new [`TxnResultMetadata`]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the is_revert
    pub fn set_is_revert(&mut self, is_revert: String) {
        self.is_revert = is_revert;
    }

    /// Set the txn_text
    pub fn set_txn_text(&mut self, txn_text: String) {
        self.txn_text = txn_text;
    }

    /// Set the txn_text_replayable
    pub fn set_txn_text_replayable(&mut self, txn_text_replayable: String) {
        self.txn_text_replayable = txn_text_replayable;
    }

    /// Set the idx
    pub fn set_idx(&mut self, idx: String) {
        self.idx = idx;
    }

    /// Set the txn_text_replayable
    pub fn set_input_type(&mut self, input_type: String) {
        self.input_type = input_type;
    }
}

impl_serdeany!(TxnResultMetadata);

/// A corpus in memory with custom storage strategies.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(bound = "I: Input")]
pub struct TxnInMemoryCorpus<I>
where
    I: Input,
{
    /// The work dir for storage
    work_dir: String,
    /// We wrapp a inmemory corpus
    inner: InMemoryCorpus<I>,
}

impl<I> UsesInput for TxnInMemoryCorpus<I>
where
    I: Input,
{
    type Input = I;
}

impl<I> Corpus for TxnInMemoryCorpus<I>
where
    I: Input,
{
    /// Returns the number of elements
    #[inline]
    fn count(&self) -> usize {
        self.inner.count()
    }

    /// Add an entry to the corpus and return its index
    #[inline]
    fn add(&mut self, testcase: Testcase<I>) -> Result<CorpusId, Error> {
        self.inner.add(testcase)
    }

    /// Replaces the testcase at the given idx
    #[inline]
    fn replace(&mut self, idx: CorpusId, testcase: Testcase<I>) -> Result<Testcase<I>, Error> {
        self.inner.replace(idx, testcase)
    }

    /// Removes an entry from the corpus, returning it if it was present.
    #[inline]
    fn remove(&mut self, idx: CorpusId) -> Result<Testcase<I>, Error> {
        self.inner.remove(idx)
    }

    /// Get by id
    #[inline]
    fn get(&self, idx: CorpusId) -> Result<&RefCell<Testcase<I>>, Error> {
        self.inner.get(idx)
    }

    /// Current testcase scheduled
    #[inline]
    fn current(&self) -> &Option<CorpusId> {
        self.inner.current()
    }

    /// Current testcase scheduled (mutable)
    #[inline]
    fn current_mut(&mut self) -> &mut Option<CorpusId> {
        self.inner.current_mut()
    }

    #[inline]
    fn next(&self, idx: CorpusId) -> Option<CorpusId> {
        self.inner.next(idx)
    }

    #[inline]
    fn prev(&self, idx: CorpusId) -> Option<CorpusId> {
        self.inner.prev(idx)
    }

    #[inline]
    fn first(&self) -> Option<CorpusId> {
        self.inner.first()
    }

    #[inline]
    fn last(&self) -> Option<CorpusId> {
        self.inner.last()
    }

    #[inline]
    fn nth(&self, nth: usize) -> CorpusId {
        self.inner.nth(nth)
    }

    #[inline]
    fn load_input_into(&self, testcase: &mut Testcase<Self::Input>) -> Result<(), Error> {
        self.inner.load_input_into(testcase)
    }

    #[inline]
    fn store_input_from(&self, testcase: &Testcase<Self::Input>) -> Result<(), Error> {
        if !unsafe { REPLAY } && testcase.has_metadata::<TxnResultMetadata>() {
            let metadata = testcase
                .metadata_map()
                .get::<TxnResultMetadata>()
                .expect("TxnResultMetadata not found");
            let idx = metadata.idx.clone();

            let data = format!(
                "Reverted? {}\n Input Type: {}\n Parent ID: {}\n Txn:\n{}",
                metadata.is_revert,
                metadata.input_type,
                testcase
                    .parent_id()
                    .as_ref()
                    .unwrap_or(&CorpusId::from(0usize))
                    .to_string(),
                metadata.txn_text
            );
            info!("============= New Corpus Item =============");
            info!("{}", data);
            info!("===========================================");

            // write to file
            let path = Path::new(self.work_dir.as_str());
            if !path.exists() {
                std::fs::create_dir_all(path).unwrap();
            }
            let mut file_path = format!("{}/{}", self.work_dir, idx);
            let mut num = 0;
            loop {
                let temp_path = format!("{}_{}", file_path, num);
                if !Path::new(file_path.as_str()).exists() {
                    break;
                } else if !Path::new(temp_path.as_str()).exists() {
                    file_path = temp_path;
                    break;
                }
                num += 1;
            }
            let mut file = File::create(file_path).unwrap();
            file.write_all(data.as_bytes()).unwrap();

            let mut replayable_file =
                File::create(format!("{}/{}_replayable", self.work_dir, idx)).unwrap();
            replayable_file
                .write_all(metadata.txn_text_replayable.as_bytes())
                .unwrap();
        }
        self.inner.store_input_from(testcase)
    }
}

impl<I> HasTestcase for TxnInMemoryCorpus<I>
where
    I: Input,
{
    fn testcase(&self, id: CorpusId) -> Result<Ref<Testcase<<Self as UsesInput>::Input>>, Error> {
        Ok(self.get(id)?.borrow())
    }

    fn testcase_mut(
        &self,
        id: CorpusId,
    ) -> Result<RefMut<Testcase<<Self as UsesInput>::Input>>, Error> {
        Ok(self.get(id)?.borrow_mut())
    }
}

impl<I> Default for TxnInMemoryCorpus<I>
where
    I: Input,
{
    fn default() -> Self {
        Self::new(String::default())
    }
}

impl<I> TxnInMemoryCorpus<I>
where
    I: Input,
{
    /// Create a new empty corpus
    pub fn new(work_dir: String) -> Self {
        Self {
            work_dir,
            inner: InMemoryCorpus::new(),
        }
    }
}
