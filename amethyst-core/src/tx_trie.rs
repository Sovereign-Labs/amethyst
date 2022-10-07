use core::panic;
use std::fmt::Debug;

use primitive_types::H256;
use revm::BlockEnv;
use serde::{Deserialize, Serialize};

use crate::verifiable_state::{EvmStateLog, OrderedRwLog};

pub trait StateCommitment: PartialEq + Debug {}
pub trait Environment: PartialEq + Debug {}

#[derive(Serialize, Deserialize)]
pub enum Transition<S, L> {
    /// An Applied Transition is just a pre and post state commitment
    Applied(S, S),
    /// A logged Transition is just an ordered set of reads and writes to be applied
    /// to a state root
    Logged(L),
    /// A hybrid Transition is a triple of a start-state, a (verifiably correct) intermediate state,
    /// and a transition log starting from the intermediate state
    Hybrid(S, S, L),
}

// TxTree is generic over a state commitment S, a transaction type Tx,
// a read-write log L,
#[derive(Deserialize, Serialize)]
pub struct TxTree<S, Tx, L, Env> {
    pub includes: Vec<Tx>,
    pub state_change: Transition<S, L>,
    pub env: Env,
}

pub type EvmTxTree<Tx> = TxTree<H256, Tx, EvmStateLog, BlockEnv>;

impl<Tx> EvmTxTree<Tx> {
    pub fn with_transactions(transactions: Vec<Tx>) -> Self {
        todo!()
    }
}

impl<S, Tx, L, Env> TxTree<S, Tx, L, Env>
where
    L: OrderedRwLog<Into = L>,
    S: StateCommitment,
    Env: Environment,
{
    pub fn merge(mut lhs: Self, rhs: Self) -> Self {
        assert_eq!(lhs.env, rhs.env);
        let transition = match (lhs.state_change, rhs.state_change) {
            (Transition::Applied(one, two), Transition::Applied(three, four)) => {
                assert_eq!(two, three);
                Transition::Applied(one, four)
            }
            (Transition::Applied(pre, mid), Transition::Logged(log)) => {
                Transition::Hybrid(pre, mid, log)
            }
            (Transition::Applied(pre, mid1), Transition::Hybrid(mid2, post, log)) => {
                assert_eq!(mid1, mid2);
                Transition::Hybrid(pre, post, log)
            }
            (Transition::Logged(_), Transition::Applied(_, _)) => {
                panic!("Must apply left-hand transition before right-hand");
            }
            (Transition::Logged(left), Transition::Logged(right)) => {
                Transition::Logged(left.merge(right))
            }
            (Transition::Logged(_), Transition::Hybrid(_, _, _)) => {
                panic!("Must apply left-hand transition before right-hand");
            }
            (Transition::Hybrid(_, _, _), Transition::Applied(_, _)) => {
                panic!("Must fully apply left-hand transition before starting right-hand");
            }
            (Transition::Hybrid(pre, post, log1), Transition::Logged(log2)) => {
                Transition::Hybrid(pre, post, log1.merge(log2))
            }
            (Transition::Hybrid(_, _, _), Transition::Hybrid(_, _, _)) => {
                panic!("Must fully apply left-hand transition before starting right-hand");
            }
        };

        lhs.includes.extend(rhs.includes.into_iter());
        TxTree {
            includes: lhs.includes,
            state_change: transition,
            env: lhs.env,
        }
    }
}
