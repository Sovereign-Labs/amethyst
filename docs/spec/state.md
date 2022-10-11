# Verifiable State

This document describes the approach to verifying and updating the "state" (accounts and storage) in Amethyst.

## Overview

Ethereum stores its state in a two-tiered Merkle-Patricia Trie (MPT). The first tier consists of a mapping between `Addresses` and `Accounts`,
while the second tier maps storage addresses to values in the context of a particular smart-contract account.

Unfortunately, emulating the MPT is quite inefficient inside of a zero-knowledge computation (zkComp) because it relies on the `Keccak256`
hash function, which makes heavy use of bitwise operations. For this reason, we want to minimize the number of MPT accesses. For additional
efficiency, we seek to batch accesses wherever possible. This allows us to share intermediate hash computations, reducing the total
number of operations to be performed.

A first step toward accomplishing this goal, would be to use the following pattern in the zkComp:

1. On first access to any state, perform a verified read (by following a Merkle path to the state root). Cache the read value.

- Excluding `SELFDESTRUCT` operations, the first _logical_ access to any piece of state within any transaction must be a `read` operation.
  1. Storage: The gas cost of SSTORE depends on the previous value, so all `writes` are implicitly read/write pairs
  1. Accounts:
  - Contract deployments are only valid if the account in question previously had `keccak256(())` as its code hash
  - Transfers _to_ an address increment (rather than overwriting) its balance - so the previous balance must be read before writing
  - Transactions _from_ an address depend on both its nonce and its balance - which must be read before the transaction can be initiated.

1. On each subsequent access, compare the `read` value to the cached value, then update the cache with the value written.
1. At the end of the execution, verifiably write the finalized state to the MPT.

However, this pattern is still not maximally efficient - it doesn't batch reads together, so many intermediate hashes will be
computed multiple times.

`SELFDESTRUCT` operations efficiently: the verifier would need to iterate over the
entire storage cache and clear each relevant entry on each `SELFDESTRUCT`, which could be expensive in practice. This specification attempts
to address these shortcomings and allow for maximally efficient state verification.

## Model

In this document, we assume the existence of a black-box EVM implementation which will produce a correct sequence of state updates given
access to some initial state. The interface between this EVM and its environment is as follows:

```typescript
interface EVM {
	function run_tx(..., db: ReadDb): Array<(Address, TrieChange)>;
}

// The EVM obtains state from the DB, which in turn fetches data from the host
interface ReadDb {
	// Note that the EVM does not have access to the storage root of accounts
	function get_account(address: H160): LimitedAccount;
	function get_storage(address: H160, slot: U256): U256;
	function get_code_by_hash(hash: H256): Array<byte>;
	function get_block_hash(address: H160, slot: U256): U256;
}

interface WriteDb {
	function apply_changes(Array<(Address, TrieChange)>);
}

// Null entries reflect "no change"
interface TrieChange {
	balance: U256 | null;
	// The code_root of the account may have changed if the account was self-destructed.
	code_root: H256 | null;
	nonce: U256 | null;;
	// If the account was self-destructed, apply the storage changes
	// onto the empty trie instead of the previous state root.
	clear_storage: bool;
	// The storage changes must be applied to calculate the new storage_root for the account
	storage_changes: Map<U256, U256>:
}
```

## Proposed Solution

Rather than verifying reads and applying writes immediately, store them in a cache-like data structure for later batch verification.
Batches can be merged together to allow efficient verification of storage access across a large number of transactions: if two batches
touch `s` and `t` items respectively, the time to merge the batches is `s + t`.

In addition to merging batches together, the cache should support running multiple transactions (seriallyO against a single underlying cache.
In other words, the cache must _not_ make any assumptions about the time at which changes will be applied.

### Data Structures

```typescript
interface RWCache {
  accounts: Map<Address, OpHistory<CachedAccountData>>;
  verified_code: Map<H256, Bytecode>;
  block_hashes: Map<number, H256>;
}

interface LimitedAccount {
  balance: U256;
  code_root: H256;
  nonce: U256;
}

interface CachedAccountData {
  account: LimitedAccount;
  base_storage_root: H256;
  storageOps: Map<U256, OpHistory<U256>>;
}

interface OpHistory<T> {
  first_read_value: T | null;
  last_written_value: T | null;
}
```

### Methods

This interface allows efficient accesses to all data necessary to validate a state transition. In the following specification, the
term "latest" is frequently used in the context of an `OpHistory<T>`. In this context, "latest" means `last_written_value` if it exists,
otherwise `first_read_value`.

### Get Account

- When `get_account` is called, check `RWCache.accounts[Address]`.
  - If an entry is found, return the account information from _latest_ cached value.
  - If there is no cached value, obtain the value of the `LimitedAccount` non-deterministically. Create a new `CachedAccountData`
    with an empty set of `storageOps` and copy the `LimitedAccount` information there. Create a new `OpHistory` with
    the `CachedAccountData` as its `first_read_value` and no `last_written_value` and store it in the cache. Return the `LimitedAccount`

### Get Storage

- When `get_storage` is called, check `RWCache.accounts[Address]`.
  - If an entry is found, check for the slot number in the _latest_ `storageOps` list.
    - If the storage slot is in cache, return the _latest_ cached value
  - Otherwise, obtain the value of the storage slot non-deterministically, create. Create a new `OpHistory` with that value as
    its`first_read_value` and no `last_written_value`.
  - Otherwise, obtain the value of the `LimitedAccount` non-deterministically. Create a new `CachedAccountData`
    with an empty set of `storageOps` and copy the `LimitedAccount` information there. Create a new `OpHistory` with
    the `CachedAccountData` as its `first_read_value` and no `last_written_value` .Read the value of the storage slot non-deterministically. Create a new `OpHistory` with that value as its`first_read_value` and no `last_written_value`, and store it
    in the storageOps list. Place the new `CachedAccountData` in the cache, and return the value of the storage slot.

### Get Code

- When `get_code_by_hash` is called, check `RWCache.verified_code[hash]`
  - If an entry is found, return it.
  - Otherwise, read the code non-deterministically. Hash it and perform `jumpdest` analysis. Store the result in cache.

TODO: consider adding an `unverified_code` cache. This would allow the elimination of expensive hashing operations when the same
code is used in multiple proofs within the same "bundle" of transactions. Since the proofs will be aggregated later, the expensive verification
can be done only once, and aggregation can perform a (much cheaper) equality check instead of hashing the code again.

### Get Blockhash

- When `get_blockhash` is called, check `RWCache.block_hashes[number]`.
  - If an entry is found, return it.
  - Otherwise, read the hash non-deterministically. Store it in the cache, and return it.

### Apply Changes

After a transaction is run, each `TrieChange` needs to be applied to the cache. Loop over the list of trie changes, fetching the cache
entry for each address. If there is no cache entry, use the logic from `get_account` to populate the `LimitedAccount` information. Construct
a new `CachedAccountData` by applying the changes from the `TrieChange` to the latest cached value.

- For `nonce`, `balance`, and `code_hash`, simply take the value from the `TrieChange` if it exists, otherwise use the cached value.
- Handling `storage` is more complicated. Here we have two cases:
  - If `clear_storage` is `false`, iterate over `TrieChange.storage_changes` and add each value into the `CachedAccountData.storageOps`
    list as the `last_written_value`, overwriting any previous `last_written_value`. In the general case, it would be ok if there was no
    `first_read_value` (the data structure is designed to handle this case), but the semantics of the EVM guarantee that every slot
    is read before being written, since gas costs vary depending on the prior value.
  - If `clear_storage` is `true`, things get a bit hairy. There are a few possible cases:
    1. This account was modified by a previous transaction against this cache, but its storage was _not_ cleared. In this case, the account's
       `base_storage_root` will not have changed between `first_read_value` and `last_written_value`
