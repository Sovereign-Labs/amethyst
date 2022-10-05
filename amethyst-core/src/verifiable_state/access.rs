use core::fmt::Debug;
#[derive(Clone)]
pub struct Modified<T> {
    pub original: Option<Box<T>>,
    pub modified: Option<Box<T>>,
}

#[derive(Clone)]
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
                // match l {
                //     Some(acct) => {
                //         assert_eq!(acct, r.original.expect("Must contain an entry"))
                //     }
                //     None => assert!(r.original.is_none()),
                // }
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
