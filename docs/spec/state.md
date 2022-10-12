# Verifiable State

This document describes the approach to verifying and updating the "state" (accounts and storage) in Amethyst.

## Background

Ethereum stores its state in a two-tiered Merkle-Patricia Trie (MPT). The first tier consists of a mapping between `Addresses` and `Accounts`,
while the second tier maps storage slot numbers to values in the context of a particular smart-contract account.

Unfortunately, emulating the MPT inside of a zero-knowledge computation (zkComp) is quite inefficient because it relies on the `Keccak256`
hash function, which makes heavy use of bitwise operations. For this reason, we want to minimize the number of MPT accesses. For additional
efficiency, we seek to "batch" accesses wherever possible. This allows us to share intermediate hash computations, reducing the total
number of operations to be performed.

A simple but inefficient method of implementating state accesses, would be to use the following pattern in the zkComp:

1. On first access to any state, perform a verified read (by following a Merkle path to the state root). Cache the read value.
   - Note: Excluding `SELFDESTRUCT` operations, the first _logical_ access to any piece of state within any transaction must be a `read` operation.
1. Storage: The gas cost of SSTORE depends on the previous value, so all `writes` are implicitly read/write pairs
1. Accounts:

   - Contract deployments are only valid if the account in question previously had `keccak256(())` as its code hash
   - Transfers _to_ an address increment (rather than overwriting) its balance - so the previous balance must be read before writing
   - Transactions _from_ an address depend on both its nonce and its balance - which must be read before the transaction can be initiated.

1. On each subsequent access, compare the `read` value to the cached value, then update the cache with the value written.
1. At the end of the execution, verifiably write the finalized state to the MPT.

However, as discussed, this pattern is far from maximally efficient. Since it doesn't batch reads together, many intermediate hashes will be
computed multiple times.

## Model

In this document, we assume the existence of a black-box EVM implementation which will produce a correct sequence of state updates given
access to some initial state. The interface between this EVM and its environment is as follows:

```typescript
interface EVM {
	function run_tx(..., db: ReadDb): Map<Address, TrieChange>;
}

// The EVM obtains state from the DB, which in turn fetches data from the host
interface ReadDb {
	// Note that the EVM does not have access to the storage root of accounts
	function get_account(address: H160): LimitedAccount;
	function get_storage(address: H160, slot: U256): U256;
	function get_code_by_hash(hash: H256): Array<byte>;
	function get_block_hash(address: H160, slot: U256): U256;
}

interface CommitDb: ReadDb {
	function apply_changes(changes: Map<Address, TrieChange>);
}

// Null entries reflect "no change"
interface TrieChange {
	balance: U256 | null;
	// The code_root of the account may have changed if the account was self-destructed.
	code_root: H256 | null;
	nonce: U256 | null;;
	// If the account was self-destructed, apply the storage changes
	// onto the empty trie instead of the previous state root.
	storage_was_cleared: bool;
	// The storage changes must be applied to calculate the new storage_root for the account
	storage_changes: Map<U256, U256>:
}
```

## Additional Constraints

In Amethyst, we have groups of transactions which are all part of a single logical "bundle" posted by a single sequencer. Given the relatively
generous per-transaction gas limits we hope to place on Amethyst, however, it may not be feasible for provers to prove entire bundles
as one large execution. For example, an attacker may be able to craft a transaction that consumes a large proportion of the RISC machine's
memory. To prevent this from becoming a permanent DOS vector, we either need to give the prover a mechanism for pruning caches (complex),
or allow bundles to be broken into smaller batches which are proven separately and recursively aggregated to prove the entire block.

To support the latter pattern, we want to create a data structure which supports efficient merges of state access information. For example,
if a user updates an account balance in the first batch of transaction, and then reads and updates it again in the second batch,
we want to be able to merge the proofs by verifying that the second read matches the first write, and then discarding both in favor of the
first read and the last write.

In addition to merging batches together, the cache should support running multiple transactions (serially) against a single underlying cache.

## Solution Overview

Rather than verifying/applying reads and writes immediately, we propose store read/write values in a cache-like data structure
for later batch verification. This structure will store the _first_ value read, and the _most-recent_ value written to each location.
Assuming the correctness of the black-box EVM implementation, these two pieces of information are sufficient to all the verification of
_all_ state accesses, and the construction of a (verified) post-state.

Using this convention, any two caches which cover consecutive transactions can be trivially combined together. Simply iterate over the storage locations in the later cache and verify that (1) the first-read value for each location matches the last-written value from the earlier cache (if present) or the first-read location (if no writes to the slot had occurred in the earlier batch). Then, overwrite the "last-written" value
from the earlier cache with the value from the later one. Using this technique, caches can be merged together very efficiently: if two caches
contain `s` and `t` items respectively, the time to merge the batches is `t`. If `t` > `s`, the technique can be reversed to allow the
merging to take place in `O(s)` time instead.

To implement this solution, we simply replace the EVM's database with one which implements the aforementioned logic, fetching
value from the (untrusted) prover when necessary.

### Data Structures

```typescript
interface RWCache {
  accounts: Map<Address, CachedAccountData>;
  verified_code: Map<H256, Bytecode>;
  block_hashes: Map<number, H256>;
}

interface LimitedAccount {
  balance: U256;
  code_root: H256;
  nonce: U256;
}

interface CachedAccountData {
  account: OpHistory<LimitedAccount>;
  storage: OpHistory<StorageIncarnation>;
}

interface StorageIncarnation {
  initial_root: H256;
  ops: Map<U256, OpHistory<U256>>;
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
  - If an entry is found, return the account information from the _latest_ cached account value.
  - If there is no cached value, obtain the value of the account - including its storage root - non-deterministically. Create a new
    `CachedAccountData` and set the `first_read_value` of the `account`. Initialize the `first_read_value` of the `storage` with the appropriate storage root and an empty set of ops. Store the new `CachedAccountData` in the cache, and return the `LimitedAccount`.
    - Note: the `storage_root` to be specified when an account is first read should be either the root as it existed at the start of the bundle unless the account was self-destructed. In that case, it should be the empty root.

### Get Storage

- When `get_storage` is called, check `RWCache.accounts[Address]`.
  - If an entry is found, check for the slot number in the _latest_ storage incarnation.
    - If the storage slot is in cache, return the _latest_ cached value
    - Otherwise, check if this account has been re-incarnated as part of the current execution. (We can do this using the fact that a
      StorageIncarnation is placed in the `last_written_value` slot of its `CachedAccountData.storage` if and only if it's been newly recreated). If so, return 0. In this case, it is _not_ necessary to persist the read into the incarnation's op history.
    - If the storage has not been re-incarnated, simply read its slot non-deterministically. Create a new `OpHistory` with that value as
      its`first_read_value` and no `last_written_value`. Return the `first_read_value`.
      - Note that incarnations which have the empty root as their `initial_root`, are _not_ guaranteed to be truly empty! It's possible that the incarnation had its storage updated by a previous transaction in the bundle but that the root hasn't been recalculated since then.
        Therefore, we only treat the storage slots as being verifiably empty if we're absolutely sure that the storage has been re-incarnated.
  - If there is no cached value, obtain the value of the account - including its storage root - non-deterministically. Create a new
    `CachedAccountData` and set the `first_read_value` of the `account`. Initialize the `first_read_value` of the `storage` with the appropriate
    storage root. Obtain the value of the storage slot non-deterministically, and place it into the op history of the newly initialized
    `storage`. Store the new `CachedAccountData` in the cache. Return the value of the storage slot.

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
entry for each address. The items must all be in cache, since there are no unconditional writes to the top-level state-trie in Ethereum.

- For `nonce`, `balance`, and `code_hash`, simply take the value from the `TrieChange` if it exists, otherwise use the cached value.
- Handling `storage` is slightly more complicated. Here we have several cases:

  1. If `storage_was_cleared` is `false`, iterate over the storage changes and insert each one into the operation history
     of the _latest_ `storage` incarnation.
  1. If `storage_was_cleared` is `true`, simply overwrite the `storage.last_written_value` with the empty root and insert a new `OpHistory` item for each slot written, using the value written as `last_written_value` and filling in 0s for the `first_read_value`s.

  - Correctness: It is always safe to overwrite the `last_written_value` of storage if the account has been `SELFDESTRUCT`ed in the most recent execution. This follows from
    the fact that we _only_ ever populate the `last_written_value` after an account has been `SELFDESTRUCT`ed. So - if the `last_written_value` was populated, that would mean
    that an account had been destroyed and recreated within the current bundle of transactions. But if the account was reincarnated in this execution, then we've already
    validated every `read` - they were all guaranteed to be zero unless they were preceded by a write in this execution - which our cache handles by default. And we don't
    need to persist any writes, since they'll simply be zeroed again by the more recent `SELFDESTRUCT`.

### Merge

To merge two caches together, use the following procedure:
terate over the `(Address, CachedAccountData)` pairs from the later of the two caches. For each item, look up the address in the _earlier_
cache. Call the two `CachedAccountData` structs `l` and `r` (for left and right).

- If `l` is null, insert `r` into the earlier cache.
- Otherwise, we need to merge the two cache entries.
  - To merge the `LimitedAccount` information...
    - Verify that `l.account.latest()` === `r.account.earliest()`.
    - If `r.account.last_written_value` is not null, set `l.account.last_written_value` = `r.account.last_written_value`.
  - To merge the `Storage` information...
    - Verify that `l.storage.latest().initial_root` === `r.storage.earliest().initial_root`.
    - For each slot "`rhs`" in `r.storage.earliest().ops`, get the corresponding slot in `l.storage.latest().ops` ("`lhs`")...
      - If `lhs` is null, set `lhs` = `rhs`. Return to the top of the loop.
      - Otherwise...
        - If `rhs.first_read_value` is not null, verify that `lhs.latest()` == `rhs.first_read_value`.
        - If `rhs.last_written_value` is not null, set `lhs.last_written_value` = `rhs.last_written_value`
    - If `r.storage.last_written_value` is not null, set `l.storage.last_written_value` = `r.storage.last_written_value`

In plain English: any time you're merging two corresponding entries `left` and `right`, verify that the latest value in `left` matches the
earliest value in `right`. If not, the prover is malicious - exit. Otherwise, recursively merge any children
of `left` and `right`, then overwrite the `last_written_value` of `left` with the one from `right`.
