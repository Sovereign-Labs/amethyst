# Amethyst

Amethyst is an equivalent zkEVM rollup, built on top of the Risc0 STARK prover. It is designed to be portable across data availability (DA)
layers, and to function as a "sovereign" rollup - one that stands on its own rather than settling to an underlying execution layer. It is
designed to operate as a ["type 1"](https://vitalik.ca/general/2022/08/04/zkevm.html) zkEVM, but it will likely be reconfigurable as a type 2
zkEVM with minimal code changes.

## Sytem Overview

The system has the following components:

1. Data Availability Layer
1. Prover
1. Sequencers
1. Full Nodes
   - Consensus/DA Client
   - Execution Client
1. Light Client

### Data Availability Layer

An underlying blockchain (or similar) is responsible for ordering and data availability. Amethyst tries to make as few assumptions about the
underlying DA layer as possible.

**_ Ordering _**
For Amethyst DA layer must establish a _total_ order over all rollup transactions. (For other rollups, a partial ordering may suffice.)

**_ Data Availability _**

For security, rollup transaction data needs to be publicly available for download. Otherwise, a malicious prover could disseminate a proof
which advanced the state of the blockchain (creating a new Merkle Root), and others would be unable to continue extending the chain
since they wouldn't be able to generate the appropriate intermediate hashes.

### The Prover

The prover runs a [Full Node](#full-nodes), which he updates by reading the transaction data off of the DA layer. He uses this full node
to supply the state data necessary to simulate the verifier and produce proofs of correctness.

When a prover is the first to post a proof that extends the canonical chain, he is rewarded with (9/10) of the base fee for all transactions
covered by his proof. For a proof to be considered valid, it needs to include a recursive verification of the proof for the previous bundle.

We treat each proof like a transaction, and we process the proof namespace before the bundle namespace:

```
             Block 1                                          Block 4
         -----------------                             ------------------
	    |   no proofs...  |                           | pf of bundle 1.1 |
	    |                 |                           | pf of bundle 1.2 |
	     -----------------                             ------------------
	    |    bundle 1.1   |                           |    bundle 4.1    |
	    |  -------------  |                           |   -------------  |
	    | |    tx1      | |            ...            |  |    tx1      | |
	    | |    tx2      | |                           |  |    tx2      | |
	    | |    ...      | |                           |  |    ...      | |
	    |  -------------  |                           |   -------------  |
        |    bundle 1.2   |                           |     bundle 4.2   |
        |      ...        |                           |       ...        |
         -----------------                             ------------------
```

### Sequencers

### Full Nodes

Full nodes consist of an Ethereum execution client coupled with a rollup-specific consensus client.

Full nodes are responsible for providing long-term retrievability of rollup data, serving light clients, and building new blocks.

### Light Clients

## Amethyst-Methods

A method is an interface to a Risc0 program. In other words, the amethyst-methods crate defines the API of the prover and verifier.

Note that although these functions are expressed in standard notation with arguments and return values, arguments are provided by the
(untrusted) prover and outputs are serialized into the proof's (tamper-proof) output log. In effect, proofs demonstrate statements of the form,
"running this function `F` on input `X` results in output `Y`".

```typescript
enum ComputationKind {
	Inline,
	Recursive,
}

interface InlineComputation {
	kind: ComputationKind.Inline;
	arguments: Array<any>;
	fn: Function;
}

interface RecursiveComputation {
	kind: ComputationKind.Recursive;
	argument_hash: H256;
	method_id: Array<byte>;
	proof:  Array<byte>;
}

interface ExecutionGuest {
	// Run each transaction, storing the reads/writes in the supplied cache and verifying that
	// the corresponding receipt is in the receipts trie.
	function run_transactions(txs: TrieRange<Transaction>, receipts: TrieRange<Receipt>, cache: RWCache);
	function run_transactions_and_output_cache(txs: TrieRange<Transaction>, receipts: TrieRange<Receipt>): (TrieRange<Transaction>, RWCache) {
		let cache = new RWCache();
		run_transactions(txs, cache);
		return (txs, cache)
	}
	// Applies the RWCache to the pre-state root to generate a new post-state root. returns a tuple (pre-state root, post-state root)
	function run_transactions_and_apply(txs: TrieRange<Transaction>, receipts: TrieRange<Receipt>, prestate: H256): (TrieRange<Transaction>, (H256, H256)) {
		let cache = new RWCache();
		run_transactions(txs, cache);
		return (txs, (pre_state, apply_state_transition(prestate, cache)));
	}
	// Returns a tuple (pre-state root, post-state root)
	// Invokes "run-transactions one or more times, and recursively aggregates the results
	// as necessary to create single RWCache (described in detail [here](./state.md)).
	// Outputs a tuple of ((pre-state root, post-state root), logsBloom, receiptsRoot, transactionsRoot)
	function run_block(block: Block, sequencer: Address, pre_state: H256): ((H256, H256), Bloom, H256, H256) ;
	// Applies the RWCache to the pre-state root to generate a new post-state root
	function apply_state_transition(initial_root: H256, state_accesses: RWCache): H256;
}

interface ExecutionHost {

}

interface Consensus {
	// Extract all rollup proofs from a Da layer block. output the hash of the DA layer block
	function extract_proofs(da_block_hash: DaBlockHash): (DaBlockHash, Array<Proof, ProverAddress>);
	// Extract all rollup blocks from a Da layer block. output the hash of the DA layer block
	function extract_blocks(da_block_hash: DaBlockHash): (DaBlockHash, Array<Block, SequencerAddress>);
	// Verifiably extract a single block rollup block from a da block, with help from the prover
	function verify_block_extraction(da_block_hash: DaBlockHash, start): (DaBlockHash, (Block, SequencerAddress))
}


```

### Run Block

The backbone of block execution is deferred computation. For performance (and DOS prevention), provers need the flexibility to
break transaction execution into several components. To accommodate this, the guest uses the following workflow.

- Create a variable `first_unprocessed_tx` and initialize it to zero. Allocate an empty `RWCache` and assign it to the variable `cache`.
  Create a variable `current_state_root` and set it equal to `block.previous_header().state_root` (where `block.previous_header()` returns a block whose header hash has been verified to match `block.prev_hash()`)
- In a loop...
  - Read a tuple `(index, ComputationStrategy)` from the host.
  - Process all transactions from `first_unprocessed_tx` up to (but not including) `index` using the provided computation strategy.
    - If the strategy is `Inline`, call `run_transactions(block.transactions[first_unprocessed_tx:index], cache)`
  - If the strategy is `Recursive`...
    - Read a proof from the host
  - Verify the proof
  - Verify that the proof's transaction range exactly matches `block.transactions[first_unprocessed_tx:index]` by checking that...
    - `proof.range.block_tx_root === block.tx_root`
    - `proof.range.start === first_unprocessed_tx` - `proof.range.end === index`
    - If the proof output contained an `RWCache`, merge that cache into the existing `cache`.
    - Otherwise, the proof output must have contained a tuple `(pre-state root, post root)`. In this case...
      - Verify that the output of `apply_state_transition(current_state_root, cache)` matches the `pre_state_root` from the proof.
      - Set `current_state_root` to equal the `post_state_root` from the proof.
      - Clear the current `RWCache`
    - If `index >= len(block.transactions)`, break the loop.
    - Otherwise, read a `StateUpdateStrategy` from the host. If the strategy is `Immediate`, set `current_state_root = apply_state_transition(current_state_root, cache)` and clear the `cache`. (We do this to allow the prover to reduce memory usage when necessary,
      at the cost of some computation).
  - Once all transactions have been processed, verify the block's bloom filter using the receipt tree.
  - Set `current_state_root = apply_state_transition(current_state_root, cache)`. Output `((block.previous().state_root, current_state_root), logsbloom, receiptsroot, transactionsRoot)`

```typescript
enum ComputationStrategy {
  Inline,
  Recursive,
}
enum StateUpdateStrategy {
  Immediate,
  Deferred,
}
interface ExecutionResult {
	gas_used: number,
	cumulative_tip: number

}

interface TrieRange<T> {
	start: number,
	end: number,
	trie_root: H256,
	// Gets the item at the requested index (with help from the prover),  verifying its position in the merkle tree
	function get_item_at(index: number): T;
	// Returns all transactions in range (with help from the prover), verifying each item
	function get_all(): Array<T>;
}
```

## The Amethyst-Core Package

Amethyst core provides the implementation of the EVM
