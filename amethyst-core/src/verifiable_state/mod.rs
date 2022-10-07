use primitive_types::{H160, H256, U256};
use revm::AccountInfo;
use serde::{Deserialize, Serialize};
type EvmAddress = H160;

mod access;
pub use access::*;
mod evm;
pub use evm::*;

/// An EvmStateEntry is a value that could exist in the Merkle-Patricia Trie
/// Since Ethereum uses a sparse merkle tree, "zero" values are not actually represented
/// in the state root. To account for this, we wrap all types in an option - if the
/// value is `None` then the item is not in the MPT and the default value should be used.
/// Otherwise, the value is actually present in the MPT.
#[derive(Serialize, Deserialize)]
pub enum EvmStateEntry {
    Accounts(EvmAddress, Option<AccountInfo>),
    Storage(EvmStorageAddress, Option<U256>),
    Blockhash(u64, Option<H256>),
}

pub trait MergeableLog: Serialize + Deserialize<'static> {
    type Into: MergeableLog;
    /// Merge two different state-access logs into a single one
    fn merge(self, rhs: Self) -> Self::Into;
}

pub trait OrderedReadLog: MergeableLog + Default {
    type State;
    fn add_read(&mut self, item: &Self::State);
}

pub trait OrderedRwLog: OrderedReadLog {
    fn add_write(&mut self, item: Self::State);
}
