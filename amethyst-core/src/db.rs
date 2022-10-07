use bytes::Bytes;
use primitive_types::{H256, U256};
use revm::{
    db::{Database, DatabaseCommit},
    AccountInfo,
};
use risc0_zkvm_guest::env;
use sha3::{Digest, Keccak256};

use crate::verifiable_state::{EvmStateEntry, EvmStorageAddress, OrderedRwLog};

pub struct HostDB<'a, L: OrderedRwLog<State = EvmStateEntry>> {
    log: &'a mut L,
}
impl<'a, L: OrderedRwLog<State = EvmStateEntry>> HostDB<'a, L> {
    pub fn from_log(log: &'a mut L) -> Self {
        Self { log }
    }
}

impl<'a, L: OrderedRwLog<State = EvmStateEntry>> DatabaseCommit for HostDB<'a, L> {
    fn commit(&mut self, changes: hashbrown::HashMap<primitive_types::H160, revm::Account>) {
        for (addr, acct) in changes.into_iter() {
            // If the account should no longer exist, simply mark its deletion and move on
            // FIXME: this is a bug - if an account is destroyed all of its storage needs to be cleared,
            // which means we need to also clear the `Storage` journal.
            if acct.is_destroyed || acct.is_empty() {
                self.log.add_write(EvmStateEntry::Accounts(addr, None));
                continue;
            }
            self.log
                .add_write(EvmStateEntry::Accounts(addr, Some(acct.info)));
            for (slot, value) in acct.storage {
                let location = EvmStorageAddress(addr, slot);
                if value.present_value().is_zero() {
                    self.log.add_write(EvmStateEntry::Storage(location, None));
                } else {
                    self.log.add_write(EvmStateEntry::Storage(
                        location,
                        Some(value.present_value()),
                    ));
                }
            }
        }
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

    // TODO: check if the value has already been read before fetching from host
    fn basic(
        &mut self,
        address: primitive_types::H160,
    ) -> Result<Option<AccountInfo>, Self::Error> {
        // Read the account from the host
        let acct: Option<AccountInfo> = env::read();

        // Don't let the host pass in unverified bytecode. Force the EVM to fetch it
        // explicity from code_by_hash instead.
        if let Some(ref info) = acct {
            assert!(
                info.code.is_none(),
                "Must pass code separately from account state!"
            )
        }
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
