use bytes::Bytes;
use primitive_types::{H160, H256, U256};
use risc0_zkvm_guest::env;
use serde::{Deserialize, Serialize};

use crate::{
    db::HostDB,
    tx_trie::TxTree,
    verifiable_state::{EvmStateEntry, EvmStateLog, OrderedReadLog, OrderedRwLog},
    Ari, SignatureValidationError,
};

pub struct EvmRollup;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionAction {
    Create,
    Call(H160),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessListItem {
    pub address: H160,
    pub slots: Vec<U256>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvmTransaction {
    pub sender: H160,
    pub body: TransactionBody,
}

impl EvmTransaction {
    pub fn add_to_env(&self, txEnv: &mut revm::TxEnv) {
        match &self.body {
            crate::ari::TransactionBody::Legacy {
                chain_id,
                nonce,
                gas_price,
                gas_limit,
                action,
                value,
                input,
            } => {
                txEnv.access_list = Vec::new();
                txEnv.caller = self.sender;
                txEnv.chain_id = *chain_id;
                txEnv.data = input.clone();
                txEnv.gas_limit = *gas_limit;
                txEnv.gas_price = *gas_price;
                txEnv.gas_priority_fee = None;
                txEnv.nonce = Some(*nonce);
                txEnv.transact_to = match action {
                    crate::ari::TransactionAction::Create => revm::TransactTo::create(),
                    crate::ari::TransactionAction::Call(addr) => revm::TransactTo::Call(*addr),
                };
                txEnv.value = *value;
            }
            crate::ari::TransactionBody::EIP2930 {
                chain_id,
                nonce,
                gas_price,
                gas_limit,
                action,
                value,
                input,
                access_list,
            } => {
                txEnv.access_list = access_list
                    .iter()
                    .map(|item| (item.address, item.slots.clone()))
                    .collect();
                txEnv.caller = self.sender;
                txEnv.chain_id = Some(*chain_id);
                txEnv.data = input.clone();
                txEnv.gas_limit = *gas_limit;
                txEnv.gas_price = *gas_price;
                txEnv.gas_priority_fee = None;
                txEnv.nonce = Some(*nonce);
                txEnv.transact_to = match action {
                    crate::ari::TransactionAction::Create => revm::TransactTo::create(),
                    crate::ari::TransactionAction::Call(addr) => revm::TransactTo::Call(*addr),
                };
                txEnv.value = *value;
            }
            crate::ari::TransactionBody::EIP1559 {
                chain_id,
                nonce,
                max_priority_fee_per_gas,
                max_fee_per_gas,
                gas_limit,
                action,
                value,
                input,
                access_list,
            } => {
                txEnv.access_list = access_list
                    .iter()
                    .map(|item| (item.address, item.slots.clone()))
                    .collect();
                txEnv.caller = self.sender;
                txEnv.chain_id = Some(*chain_id);
                txEnv.data = input.clone();
                txEnv.gas_limit = *gas_limit;
                txEnv.gas_price = *max_fee_per_gas;
                txEnv.gas_priority_fee = Some(*max_priority_fee_per_gas);
                txEnv.nonce = Some(*nonce);
                txEnv.transact_to = match action {
                    crate::ari::TransactionAction::Create => revm::TransactTo::create(),
                    crate::ari::TransactionAction::Call(addr) => revm::TransactTo::Call(*addr),
                };
                txEnv.value = *value;
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionBody {
    Legacy {
        chain_id: Option<u64>,
        nonce: u64,
        gas_price: U256,
        gas_limit: u64,
        action: TransactionAction,
        value: U256,
        input: Bytes,
    },
    EIP2930 {
        chain_id: u64,
        nonce: u64,
        gas_price: U256,
        gas_limit: u64,
        action: TransactionAction,
        value: U256,
        input: Bytes,
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
        input: Bytes,
        access_list: Vec<AccessListItem>,
    },
}

impl Ari for EvmRollup {
    type Address = H160;
    type StateCommitment = H256;
    type Transaction = EvmTransaction;
    type StateEntry = EvmStateEntry;
    type Environment = revm::BlockEnv;

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

    /// We give provers flexibility to treat the verification of each transaction as
    /// a separate proof and recursively aggregate them (when necessary), but encourage
    /// them to compute as many transactions as possible in one long execution.
    ///
    /// Longer executions take advantage of Risc0's transparent recursion/parallelism, which
    /// simplifies the proving process. However, very long executions could easily run out of memory
    /// in the guest, so we allow each transaction to be processed separately to prevent attacks.
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
        let mut sub_tries: Vec<
            TxTree<Self::StateCommitment, Self::Transaction, L, Self::Environment>,
        > = Vec::new();
        for tx in bundle {
            let existing_trie: Option<TxTree<_, _, _, _>> = env::read();
            if let Some(trie) = existing_trie {
                sub_tries.push(trie);
                continue;
            }
        }
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
