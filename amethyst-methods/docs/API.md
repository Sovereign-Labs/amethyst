# Stratum V0

In this document, we attempt to translate the high-level requirements sketched in [DESIGN.md](./DESIGN.md) into a set of methods to be implemented by all validity-proven rollups. This is conceptually similar to ABCI, though the actual implementation may depart significantly from that standard due to the difference in application.

## Methods

1. `FindAllBundles(DaBlock: &[u8]) -> IntoIterator<(Address, &[u8])>`. TODO
1. `PreValidateBundle(signer: Address, bundle: &'a [u8]) -> Result<(Address, &'a [u8]), PreValidationError>` Verifies that the signer is a registered sequencer and is allowed to post bundles at least as large as bundle (i.e. has sufficient funds, bundle is not too large). Does not attempt to slash sequencer on failure, since this check is nearly free and many sequencers will be completely unbonded.
1. `ValidateBundle(signer: Address, bundle: &[u8]) -> Result<(SequencerState, Bundle)>`. deserializes the bundle, charging the sequencer for each transaction. Returns an error if the bundle is invalid. If ErrNotASequencer, ignore. Otherwise, slash the sequencer
1. `ValidateTransactions(bundle: Bundle, purported_state: impl StateProvider) -> (RWLog, Iterator<Item=ApplicableTransaction>)`. Verifies that each transaction is semantically valid (i.e. nonce, gas price) given current rollup state. There are no errors, since invalid transactions are merely filtered out of the iterator.
1. `ExecuteTransactions(read_write_log: &mut RWLog, transactions: Iterator<Item=ApplicableTransaction>) -> Result<()>`. Apply the transactions. If any transactions, fail returns an error, indicating that the sequencer should be slashed
1. `ExecuteTransaction(tx: TxOrFuture)`: Note: this function should be flexible enough to allow either local execution or dispatch of this job.
1. `ApplyRwLog(read_write_log: &mut RWLog) -> Result<Option<Hash>>`. Apply the rw log, creating a new state root. If this function returns an error, the proof is invalid. If it returns none, the sequencer was slashed
