use std::fmt::Debug;

use primitive_types::H256;

use crate::{
    ari::EvmTransaction,
    db::HostDB,
    tx_trie::{Environment, TxTree},
    verifiable_state::{EvmStateLog, OrderedRwLog},
};

pub trait Transaction
where
    Self: Sized,
{
    type Env: PartialEq + Debug;
    type StateCommitment;
    type Log: OrderedRwLog;

    fn run_standalone(
        &self,
        env: Self::Env,
        log: &mut Self::Log,
    ) -> TxTree<Self::StateCommitment, Self, Self::Log, Self::Env>;
}

impl Transaction for EvmTransaction {
    type Env = revm::BlockEnv;
    type Log = EvmStateLog;
    type StateCommitment = H256;

    fn run_standalone(
        &self,
        env: Self::Env,
        log: &mut Self::Log,
    ) -> TxTree<H256, Self, Self::Log, Self::Env> {
        let mut rw_log = Self::Log::default();
        let db = HostDB::from_log(&mut rw_log);
        let mut evm = revm::new::<HostDB<Self::Log>>();

        self.add_to_env(&mut evm.env.tx);
        evm.env.block = env;
        evm.env.cfg = revm::CfgEnv::default();
        // TODO: validate the sequencer ID!
        if let Some(sequencer_id) = evm.env.tx.chain_id {
            evm.env.cfg.chain_id = sequencer_id.into();
        }

        //  {
        //     chain_id: todo!(),
        //     spec_id: revm::SpecId::LATEST,
        //     perf_all_precompiles_have_balance: false,
        //     perf_analyse_created_bytecodes: revm::AnalysisKind::Raw,
        //     limit_contract_code_size: todo!(),
        // };

        // evm.env = revm::Env {
        //     tx: revm::TxEnv {
        //         caller: self.sender,
        //         gas_limit: ,
        //         gas_price: (),
        //         gas_priority_fee: (),
        //         transact_to: (),
        //         value: (),
        //         data: (),
        //         chain_id: (),
        //         nonce: (),
        //         access_list: (),
        //     },
        //     block: todo!(),
        //     cfg: todo!(),
        // };

        evm.transact_commit();

        todo!()
    }
}
