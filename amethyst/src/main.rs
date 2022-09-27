fn main() {}

#[cfg(test)]
mod tests {
    use amethyst_methods::{MAIN_ID, MAIN_PATH};
    use risc0_zkvm::host::{Prover, ProverOpts, Receipt};
    use risc0_zkvm::serde::{from_slice, to_vec};

    #[cfg(feature = "receipt")]
    fn add_receipt_options(opts: ProverOpts) -> ProverOpts {
        opts.with_skip_seal(false)
    }

    #[cfg(not(feature = "receipt"))]
    fn add_receipt_options(opts: ProverOpts) -> ProverOpts {
        opts.with_skip_seal(true)
    }

    #[cfg(feature = "receipt")]
    fn verify_receipt(receipt: Receipt, method_id: &[u8]) {
        receipt.verify(method_id).unwrap()
    }
    #[cfg(not(feature = "receipt"))]
    fn verify_receipt(receipt: Receipt, method_id: &[u8]) {}

    #[test]
    fn run_prover_with_receipt() {
        let a: u64 = 1471;
        let b: u64 = 131;

        let opts = add_receipt_options(ProverOpts::default());
        let mut prover =
            Prover::new_with_opts(&std::fs::read(MAIN_PATH).unwrap(), MAIN_ID, opts).unwrap();

        prover.add_input(to_vec(&a).unwrap().as_slice()).unwrap();
        prover.add_input(to_vec(&b).unwrap().as_slice()).unwrap();
        // Run prover & generate receipt
        let receipt = prover.run().unwrap();

        // Extract journal of receipt (i.e. output c, where c = a * b)
        let c: u64 = from_slice(&receipt.get_journal_vec().unwrap()).unwrap();

        // Print an assertion
        println!("I know the factors of {}, and I can prove it!", c);

        // Here is where one would send 'receipt' over the network...

        // Verify receipt, panic if it's wrong
        verify_receipt(receipt, MAIN_ID)
    }
}
