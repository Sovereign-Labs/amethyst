#![feature(entry_insert)]
use risc0_zkvm::host::Receipt;

pub mod db;
pub mod tx;
pub mod verifiable_state;
use serde::{Deserialize, Serialize};
use verifiable_state::{OrderedReadLog, OrderedRwLog};

pub enum Job {
    Todo,
    Delegated,
}

pub enum BundlePrevalidationError {
    InsufficientFunds,
    NotASequencer,
}

pub enum DeserializationError {}

#[derive(Serialize, Deserialize)]
pub enum SignatureValidationError {
    Any,
}

type MethodId = &'static [u8];

pub enum ComputationTree {
    LeafNode(MethodId),
    InternalNode(MethodId, Receipt),
}

pub struct Bundle;

pub trait SequencerState {
    /// An address in the underlying DA layer
    type Address;
    /// The base unit of the currency used for bonding sequencers
    type Units;
    /// Returns the balance of a given sequencer
    fn balance_of(address: Self::Address) -> Self::Units;
    /// Returns true if and only if the sequencer is currently eligible based on their outstanding balance
    fn is_eligible(address: Self::Address) -> bool;
    /// Increment the sequencer's balance by the provided amount. Returns the new balance
    fn increase_balance(address: Self::Address, amount: Self::Units) -> Self::Units;
    /// Decrease the sequencer's balance by the provided amount. Returns the new balance
    fn decrease_balance(address: Self::Address, amount: Self::Units) -> Self::Units;
}

pub trait Ari {
    type Transaction;
    type Address;
    type StateCommitment;

    // TODO: the interface of this method will change
    fn next_bundle<'a, I: Iterator<Item = (Self::Address, &'a [u8])>>(bytes: &'a [u8]) -> I;

    /// Validates that the sequencer is registered and has sufficient funds to pay for the bundle's bytes
    fn prevalidate_bundle<L: OrderedReadLog>(
        sequencer: Self::Address,
        bytes: &[u8],
        read_log: L,
    ) -> Result<&[u8], BundlePrevalidationError>;

    /// Deserializes the raw bundle into a list of transactions
    fn deserialize_bundle<L: OrderedReadLog>(
        sequencer: Self::Address,
        bytes: &[u8],
        rw_log: L,
    ) -> Result<Bundle, DeserializationError>;

    fn filter_transactions<L: OrderedRwLog, I: Iterator<Item = Self::Transaction>>(
        bundle: Bundle,
        rw_log: L,
    ) -> I;

    /// Applies all transactions, updating the state (including the sequencer balance) as necessary via the RwLog
    fn apply_transactions<L: OrderedRwLog, I: Iterator<Item = Self::Transaction>>(
        bundle: I,
        rw_log: L,
    ) -> Result<(), SignatureValidationError>;

    /// Executes a transaction and adds its state into the RW Log
    fn execute_transaction<L: OrderedRwLog>(
        tx: Self::Transaction,
    ) -> Result<L, SignatureValidationError>;

    /// Verifies execution of a transaction and merges its state into the RW Log
    fn verify_transaction<L: OrderedRwLog>(
        tx: Self::Transaction,
        rw_log: L,
    ) -> Result<(), SignatureValidationError>;

    fn apply_rw_log<L: OrderedRwLog>(
        prev_state_commit: Self::StateCommitment,
        rw_log: L,
    ) -> Self::StateCommitment;
}
