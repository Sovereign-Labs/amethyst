use core::fmt::Debug;

use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
pub struct Modified<T> {
    pub original: Option<Box<T>>,
    pub modified: Option<Box<T>>,
}

#[derive(Clone, Serialize, Deserialize)]
/// `Access` represents a sequence of events on a particular value.
/// For example, a transaction might read a value, then take some action which causes it to be updated
/// The rules for defining causality are as follows:
/// 1. If a read is preceded by another read, check that the two reads match and discard one.
/// 2. If a read is preceded by a write, check that the value read matches the value written. Discard the read.
/// 3. Otherwise, retain the read.
/// 4. A write is retained unless it is followed by another write.
pub enum Access<T> {
    // In the EVM, empty accounts are not represented in the MPT
    // """ The final state, σ, is reached after deleting all accounts
    // that either appear in the self-destruct set or are touched and empty """
    Read(Option<Box<T>>),
    ReadThenWrite(Modified<T>),
    Write(Option<Box<T>>),
}

impl<T> Access<T>
where
    T: PartialEq + Debug,
{
    pub fn merge(self, rhs: Self) -> Self {
        match (self, rhs) {
            (Access::Read(l), Access::Read(r)) => {
                assert_eq!(l, r);
                Access::Read(l)
            }
            (Access::Read(l), Access::ReadThenWrite(r)) => {
                assert_eq!(l, r.original);
                Access::ReadThenWrite(r)
            }
            (Access::Read(l), Access::Write(r)) => Access::ReadThenWrite(Modified {
                original: l,
                modified: r,
            }),
            (Access::ReadThenWrite(l), Access::Read(r)) => {
                assert_eq!(l.modified, r);
                Access::ReadThenWrite(l)
            }
            (Access::ReadThenWrite(mut l), Access::ReadThenWrite(r)) => {
                assert_eq!(l.modified, r.original);
                l.modified = r.modified;
                Access::ReadThenWrite(l)
            }
            (Access::ReadThenWrite(mut l), Access::Write(r)) => {
                l.modified = r;
                Access::ReadThenWrite(l)
            }
            (Access::Write(l), Access::Read(r)) => {
                assert_eq!(l, r);
                Access::Write(l)
            }
            (Access::Write(l), Access::ReadThenWrite(r)) => {
                assert_eq!(l, r.original);
                Access::Write(r.modified)
            }
            (Access::Write(_), Access::Write(r)) => Access::Write(r),
        }
    }
}
