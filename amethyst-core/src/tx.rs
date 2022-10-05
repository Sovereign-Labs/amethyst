use primitive_types::{H160, H256, U256};
use risc0_zkvm_guest::env;

use crate::{
    db::HostDB,
    verifiable_state::{EvmStateEntry, EvmStateLog, OrderedReadLog, OrderedRwLog},
    Ari, SignatureValidationError,
};

pub struct EvmRollup;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionAction {
    Create,
    Call(H160),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessListItem {
    pub address: H160,
    pub slots: Vec<U256>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvmTransaction {
    sender: H160,
    body: TransactionBody,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransactionBody {
    Legacy {
        chain_id: Option<u64>,
        nonce: u64,
        gas_price: U256,
        gas_limit: u64,
        action: TransactionAction,
        value: U256,
        input: Vec<u8>,
    },
    EIP2930 {
        chain_id: u64,
        nonce: u64,
        gas_price: U256,
        gas_limit: u64,
        action: TransactionAction,
        value: U256,
        input: Vec<u8>,
        access_list: Vec<AccessListItem>,
    },
    EIP1559 {
        chain_id: u64,
        nonce: u64,
        max_priority_fee_per_gas: U256,
        max_fee_per_gas: U256,
        gas_limit: u64,
        action: TransactionAction,
        value: U256,
        input: Vec<u8>,
        access_list: Vec<AccessListItem>,
    },
}

impl Ari for EvmRollup {
    type Address = H160;
    type StateCommitment = H256;
    type Transaction = EvmTransaction;
    type StateEntry = EvmStateEntry;

    fn next_bundle<'a, I: Iterator<Item = (Self::Address, &'a [u8])>>(bytes: &'a [u8]) -> I {
        todo!()
    }

    fn prevalidate_bundle<L: OrderedReadLog>(
        sequencer: Self::Address,
        bytes: &[u8],
        read_log: L,
    ) -> Result<&[u8], crate::BundlePrevalidationError> {
        todo!()
    }

    fn deserialize_bundle<L: OrderedReadLog>(
        sequencer: Self::Address,
        bytes: &[u8],
        rw_log: L,
    ) -> Result<crate::Bundle, crate::DeserializationError> {
        todo!()
    }

    fn filter_transactions<L: OrderedRwLog, I: Iterator<Item = Self::Transaction>>(
        bundle: crate::Bundle,
        rw_log: L,
    ) -> I {
        todo!()
    }

    fn execute_transaction<L: OrderedRwLog<State = EvmStateEntry>>(
        tx: Self::Transaction,
    ) -> Result<L, SignatureValidationError> {
        let sender: Result<H160, SignatureValidationError> = env::read();
        // TODO: actually validate signatures
        let sender = match sender {
            Ok(s) => s,
            Err(e) => return Err(e),
        };
        assert_eq!(sender, tx.sender);

        // TODO: get size hints from the prover
        let mut rw_log = L::default();
        let db = HostDB::from_log(&mut rw_log);
        let evm = revm::new::<HostDB<L>>();

        Ok(rw_log)
    }

    fn apply_transactions<L: OrderedRwLog, I: Iterator<Item = Self::Transaction>>(
        bundle: I,
        rw_log: L,
    ) -> Result<(), crate::SignatureValidationError> {
        todo!()
    }

    fn verify_transaction<L: OrderedRwLog>(
        tx: Self::Transaction,
        rw_log: L,
    ) -> Result<(), crate::SignatureValidationError> {
        todo!()
    }

    fn apply_rw_log<L: OrderedRwLog>(
        prev_state_commit: Self::StateCommitment,
        rw_log: L,
    ) -> Self::StateCommitment {
        todo!()
    }
}
