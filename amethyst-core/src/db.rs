use bytes::Bytes;
use primitive_types::{H256, U256};
use revm::{db::Database, AccountInfo};
use risc0_zkvm_guest::env;
use sha3::{Digest, Keccak256};

use crate::verifiable_state::{
    EvmStateEntry, EvmStateLog, EvmStorageAddress, OrderedReadLog, OrderedRwLog,
};

pub struct HostDB<'a, L: OrderedRwLog<State = EvmStateEntry>> {
    log: &'a mut L,
}
impl<'a, L: OrderedRwLog<State = EvmStateEntry>> HostDB<'a, L> {
    pub fn from_log(log: &'a mut L) -> Self {
        Self { log }
    }
}

// TODO: swap for optimized implementation
fn keccak256(bytes: &[u8]) -> H256 {
    H256::from_slice(Keccak256::digest(&bytes).as_slice())
}

pub enum HostDBError {
    InvalidBlockHashRequest,
}

impl<'a, L: OrderedRwLog<State = EvmStateEntry>> Database for HostDB<'a, L> {
    type Error = HostDBError;

    fn basic(
        &mut self,
        address: primitive_types::H160,
    ) -> Result<Option<AccountInfo>, Self::Error> {
        let acct: Option<AccountInfo> = env::read();
        self.log
            .add_read(&EvmStateEntry::Accounts(address, acct.clone()));
        Ok(acct)
    }

    fn code_by_hash(
        &mut self,
        code_hash: primitive_types::H256,
    ) -> Result<revm::Bytecode, Self::Error> {
        let raw_code: Bytes = env::read();
        // TODO: Swap hash function implementation
        // Safety: we've computed the hash ourselves
        let computed_hash = keccak256(&raw_code);
        assert_eq!(computed_hash, code_hash);
        let bytecode = unsafe { revm::Bytecode::new_raw_with_hash(raw_code, computed_hash) };
        Ok(bytecode)
    }

    fn storage(
        &mut self,
        address: primitive_types::H160,
        index: primitive_types::U256,
    ) -> Result<primitive_types::U256, Self::Error> {
        let val: Option<U256> = env::read();
        self.log.add_read(&EvmStateEntry::Storage(
            EvmStorageAddress(address, index),
            val,
        ));
        Ok(val.unwrap_or_default())
    }

    fn block_hash(
        &mut self,
        number: primitive_types::U256,
    ) -> Result<primitive_types::H256, Self::Error> {
        let val: Option<H256> = env::read();
        self.log
            .add_read(&EvmStateEntry::Blockhash(number.as_u64(), val));
        val.ok_or(HostDBError::InvalidBlockHashRequest)
    }
}
