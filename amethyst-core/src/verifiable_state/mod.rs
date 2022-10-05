use std::{
    collections::{hash_map::Entry, HashMap},
    fmt::Debug,
};

use primitive_types::{H160, U256};
use revm::AccountInfo;
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
pub enum EvmStateEntry {
    Accounts(EvmAddress, Option<AccountInfo>),
    Storage(EvmStorageAddress, Option<U256>),
}

pub trait MergeableLog {
    type Into: MergeableLog;
    /// Merge two different state-access logs into a single one
    fn merge(self, rhs: Self) -> Self::Into;
}

pub trait OrderedReadLog: MergeableLog {
    type State;
    fn new() -> Self;
    fn add_read(&mut self, item: &Self::State);
}

pub trait OrderedRwLog: OrderedReadLog {
    fn add_write(&mut self, item: Self::State);
}
