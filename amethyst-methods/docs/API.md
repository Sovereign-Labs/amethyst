# Stratum V0

In this document, we attempt to translate the high-level requirements sketched in [DESIGN.md](./DESIGN.md) into a set of methods to be implemented by all validity-proven rollups. This is conceptually similar to ABCI, though the actual implementation may depart significantly from that standard due to the difference in application.

## Methods

`FindAllBundles(DaBlock: &[u8]) -> IntoIterator<(Address, &[u8])>`. TODO
`ValidateBundle(signer: Address, bundle: &[u8]) -> Result<(SequencerState, Bundle)>`. Verifies that the signer is a valid sequencer and deserializes the bundle, charging the sequencer for each transaction. Returns an error if the bundle is invalid. If ErrNotASequencer, ignore. Otherwise, slash the sequencer
`ValidateTransactions(bundle: Bundle, purported_state: impl StateProvider) -> (RWLog, Iterator<Item=ApplicableTransaction>)`. Verifies that each transaction is semantically valid (i.e. nonce, gas price) given current rollup state. There are no errors, since invalid transactions are merely filtered out of the iterator.
`ExecuteTransactions(read_write_log: &mut RWLog, transactions: Iterator<Item=ApplicableTransaction>) -> Result<()>`. Apply the transactions. If any transactions, fail returns an error, indicating that the sequencer should be slashed
`ExecuteTransaction(tx: TxOrFuture)`: Note: this function should be flexible enough to allow either local execution or dispatch of this job.
`ApplyRwLog(read_write_log: &mut RWLog) -> Result<Option<Hash>>`. Apply the rw log, creating a new state root. If this function returns an error, the proof is invalid. If it returns none, the sequencer was slashed
