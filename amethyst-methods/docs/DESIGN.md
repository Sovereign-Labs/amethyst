## Amethyst Methods

This package defines the methods necessary to fully validate the execution of an EVM rollup.

Validation can happen at four levels of granularity:

- _slot_ - a "slot" (aka DA layer block) can contain multiple bundles of transactions, each submitted by different sequencers
- _bundle_ - a "bundle" is a group of one or more transactions submitted to the DA layer by a sequencer
- _transaction_ - a transaction is the basic unit of the state-transition function.
- _proof_ - a zero-knowledge proof of a group of one or more bundles

### Short Circuiting

A design goal of amethyst is to allow for the fast generation of proofs of _invalidity_. As such, we impose the following short-circuit conditions (in-order!!):

- When a _bundle_ is submitted by a sequencer who does not have a large enough bond, ignore the bundle.
- When a _bundle_ includes a transaction that is _structurally invalid_ **_excluding signature checks_**, slash the sequencer who posted the bundle (immediately - so that any subsequent bundles posted by the sequencer will also be ignored) and ignore all transactions included within the structurally invalid bundle.
  - A transaction is considered to be "structurally invalid" if it contains an error that can be detected without reference to the current rollup state (illegal serialization, ...).
- When a _transaction_ is "semantically invalid", skip it without checking the signature. "Semantically invalid" transactions are ones which can be judged to be invalid without simulating them entirely. For example, a tx is semantically invalid if the originating account cannot afford to pay for `gas_price * gas_limit` or the if the nonce is incorrect. Do not slash sequencers for including semantically invalid transactions.
- When a _transaction_ has an invalid signature (after having passed the other checks) slash the sequencer and do not continue validating subsequent transactions. Revert any changes caused by previous transactions in the bundle.
- When a _proof_ is submitted by a prover who does not have a large enough bond, ignore the proof.
- When a _proof_ is structurally invalid, slash the prover and ignore the proof
  - A proof is structurally invalid if it contains an error that can be detected without reference to the current rollup state - for example, if it doesn't contain some required output in its commit log, or if the bundle it references does not exist
- When a _proof_ is semantically invalid - meaning that it purports to prove a bundle which has already been handled by an earlier proof - don't validate the proof.
- Only if a proof is structurally and semantically valid should it actually be executed. If it fails to verify, slash the prover.

Important notes:

- If a prover includes an invalid proof but someone else has already (validly) proven the bundle in question, he will not be slashed! This is ok, since he is already penalized by having to pay the DA fee for his proof and the effort he imposes on the network is near-zero. We make this design decision for efficiency reasons; we want to be able skip the expensive recursive validation of proofs unless they actually extend the chain. If we tried to punish provers who submitted invalid proofs for old blocks, we would have to verify _all_ proofs even if they were irrelevant.
- If a sequencer includes a semantically invalid transaction, we don't slash him. This is important, since several sequencers may attempt to post the same transaction at roughly the same time. However, we may want to charge sequencers a small fee per-transaction to cover the cost of (verifiably) checking semantic validity and deserializing even in cases where the signature checks and execution are skipped.

### Bundle Validation Workflow

With the previous short-circuiting rules in mind, we propose the following verification sequence:

1. Check that the _sequencer_ is validly bonded.
1. Deserialize the bundle. If deserialization fails, slash the sequencer and exit.
1. Subtract `MIN_PROVING_COST * len(bundle.transactions)` from the sequencer's bond. If the sequencer's new balance is below the minimum, do _not_ slash them further: simply transfer `MIN_PROVING_COST * len(bundle.transactions)` to the prover and exit.
1. **If you make it to this point, you MUST validate the read log of the bundle against the state root before exiting.** Iterate over the transactions, checking the nonce and gas limits non-deterministically. (Important: The `read` operations must be placed in the verifiable journal from the proof for later verification - see [here](https://www.notion.so/Efficiency-Improvements-for-Batch-Transaction-Processing-41040e279aee49ee8bce674812f26b72) for more details)
1. Iterate over the remaining transactions, checking the signatures and executing the transactions. Be sure to update the read/write log from the proof journal as you go. If any signature checks fail, slash the sequencer and break the loop.
1. Validate the read/write log and generate a new state root. _If the log validation fails, the proof is invalid._
1. If the sequencer was slashed, transfer half of their bond to the prover, burn the other half, output the _old_ state root, and exit. Otherwise, output the "write" portion of the log and the new state root.

### Additional Fees

Due to the lack of coordination among sequencers, we expect many duplicate transactions. Since these transactions use no gas, provers don't get paid for processing them. Therefore, we would like to minimize the work done in processing such transactions. One possible optimization would be to avoid checking the signature if a transaction has an invalid nonce or gas limit. Unfortunately, this introduces a problem when the signature is _invalid_. In order to prove that the signature check failed, a prover first has to demonstrate that the nonce/gas check succeeded - which requires proving every previous transaction.

But, since the signature check only fails very rarely (and sense the prover is well-compensated in that case by their share of the sequencer's bond), this is not a big issue.

### A Quick Note on MEV

If you have open sequencing, (in the limit) the DA layer proposer will extract all MEV on the rollup. Why? A smart DA layer block proposer will simply read all of the incoming rollup transactions, and then include his own transaction that extracts the value from any good bundles. This _might_ be a good thing - the l2 effictively subsidizes the security of l1. On the other hand, this is technical censorship and introduces some risk of real censorship. To mitigate this, we would need some combination of...

1. L2 sequencers with private order flow - so that DA layer proposers can't just watch the l2 mempool the best blocks on their own
1. Private DA transactions - so that DA layer proposer can't just watch the L1 mempool and steal good bundles. These can be implemented using threshold encryption (preferable) or a VDF (not ideal)
