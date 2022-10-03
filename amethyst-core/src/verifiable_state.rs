use std::collections::{hash_map::Entry, HashMap};

use primitive_types::{H160, H256, U256};
use revm::AccountInfo as Account;

type Address = H160;

#[derive(Clone)]
pub struct ModifiedAccount {
    original: Option<Account>,
    modified: Account,
}

#[derive(Clone)]
pub enum StateAccess {
    Read(Option<Box<Account>>),
    ReadThenWrite(Box<ModifiedAccount>),
    Write(Box<Account>),
}

impl StateAccess {
    pub fn merge(self, rhs: Self) -> Self {
        match (self, rhs) {
            (StateAccess::Read(l), StateAccess::Read(r)) => {
                assert_eq!(l, r);
                StateAccess::Read(l)
            }
            (StateAccess::Read(l), StateAccess::ReadThenWrite(r)) => {
                match l {
                    Some(acct) => {
                        assert_eq!(&*acct, r.original.as_ref().expect("Must contain an entry"))
                    }
                    None => assert!(r.original.is_none()),
                }
                StateAccess::ReadThenWrite(r)
            }
            (StateAccess::Read(l), StateAccess::Write(r)) => {
                StateAccess::ReadThenWrite(Box::new(ModifiedAccount {
                    original: match l {
                        Some(acct) => Some(*acct),
                        None => None,
                    },
                    modified: *r,
                }))
            }
            (StateAccess::ReadThenWrite(l), StateAccess::Read(r)) => {
                assert_eq!(l.modified, *r.expect("Must contain an entry"));
                StateAccess::ReadThenWrite(l)
            }
            (StateAccess::ReadThenWrite(mut l), StateAccess::ReadThenWrite(r)) => {
                assert_eq!(l.modified, r.original.expect("Must contain an entry"));
                l.modified = r.modified;
                StateAccess::ReadThenWrite(l)
            }
            (StateAccess::ReadThenWrite(mut l), StateAccess::Write(r)) => {
                l.modified = *r;
                StateAccess::ReadThenWrite(l)
            }
            (StateAccess::Write(l), StateAccess::Read(r)) => {
                assert_eq!(*l, *r.expect("Must contain an entry"));
                StateAccess::Write(l)
            }
            (StateAccess::Write(l), StateAccess::ReadThenWrite(r)) => {
                assert_eq!(*l, r.original.expect("Must contain an entry"));
                StateAccess::Write(Box::new(r.modified))
            }
            (StateAccess::Write(_), StateAccess::Write(r)) => StateAccess::Write(r),
        }
    }
}

#[derive(Default)]
pub struct LeafStateLog {
    pub accounts: HashMap<Address, StateAccess>,
    pub state: HashMap<(Address, U256), U256>,
}

pub trait MergeableLog {
    type Into: MergeableLog;
    /// Merge two different state-access logs into a single one
    fn merge(self, rhs: Self) -> Self::Into;
}

pub trait OrderedReadLog: MergeableLog {
    fn new() -> Self;
    fn add_read(&mut self, addr: &Address, value: &Option<Account>);
}

pub trait OrderedRwLog: OrderedReadLog {
    fn add_write(&mut self, addr: &Address, value: Account);
}

impl MergeableLog for LeafStateLog {
    type Into = Vec<(Address, StateAccess)>;
    /// Merges the read and write logs of two separate proofs to create a master log with the minimal amount of information
    /// necessary to verify both proofs against the old state commitment and compute the new one.
    fn merge(mut self, rhs: Self) -> Self::Into {
        let mut output = Vec::with_capacity(rhs.accounts.len() + self.accounts.len());
        for (addr, right) in rhs.accounts.into_iter() {
            let combined_entry = if let Some(left) = self.accounts.remove(&addr) {
                left.merge(right)
            } else {
                right
            };

            output.push((addr, combined_entry))
        }
        for item in self.accounts.into_iter() {
            output.push(item)
        }
        // TODO: improve efficiency from N log(N) to O(N) with hints from the host.
        output.sort_by(|(left_addr, _), (right_addr, _)| left_addr.cmp(right_addr));
        output
    }
}

impl MergeableLog for Vec<(Address, StateAccess)> {
    type Into = Self;
    fn merge(self, rhs: Self) -> Self::Into {
        let mut output = Vec::with_capacity(self.len() + rhs.len());
        let mut rhs = rhs.into_iter();
        let mut lhs = self.into_iter();

        let mut left = lhs.next();
        let mut right = rhs.next();
        loop {
            match (left, right) {
                // If both iterators are exhausted, we've processed every element. Return the output
                (None, None) => return output,
                // If one iterator is exhausted, we don't need to do any more work. Just put any remaining items
                // from the other iterator into the output and return
                (None, Some(r)) => {
                    output.push(r);
                    output.extend(rhs);
                    return output;
                }
                (Some(l), None) => {
                    output.push(l);
                    output.extend(lhs);
                    return output;
                }
                // If both iterators are still populated, compare the first item from each.
                (Some(l), Some(r)) => match l.0.cmp(&r.0) {
                    // If they don't match, advance the iterator that's behind.
                    // Since the two locations are different, we don't need to do any merging.
                    // Simply pop the lower item and throw it into the output array
                    std::cmp::Ordering::Less => {
                        output.push(l);
                        left = lhs.next();
                        right = Some(r);
                    }
                    // If the two storage locations *do* match, then we need to merge the accesses.
                    // Do that, then advance both iterators.
                    std::cmp::Ordering::Equal => {
                        output.push((l.0, l.1.merge(r.1)));
                        left = lhs.next();
                        right = rhs.next();
                    }
                    // If the right-hand iterator is behind, advance it using the mirror of the logic
                    // above
                    std::cmp::Ordering::Greater => {
                        output.push(r);
                        left = Some(l);
                        right = rhs.next();
                    }
                },
            }
        }
    }
}

impl OrderedReadLog for LeafStateLog {
    fn new() -> Self {
        Self::default()
    }

    fn add_read(&mut self, addr: &Address, value: &Option<Account>) {
        match self.accounts.entry(*addr) {
            Entry::Occupied(existing) => {
                match existing.get() {
                    // If we've already read this slot, ensure that the two reads match, then discard one.
                    StateAccess::Read(r) => match r {
                        Some(acct) => {
                            assert_eq!(&**acct, value.as_ref().expect("Must contain a value"))
                        }
                        None => assert!(value.is_none()),
                    },
                    // If this slot has already been written, ensure that the value we just read is the one previously written
                    StateAccess::ReadThenWrite(m) => {
                        assert_eq!(&m.modified, value.as_ref().expect("Must contain a value"))
                    }
                    StateAccess::Write(w) => {
                        assert_eq!(&**w, value.as_ref().expect("Must contain a value"))
                    }
                };
            }
            Entry::Vacant(v) => {
                match value {
                    Some(acct) => v.insert_entry(StateAccess::Read(Some(Box::new(acct.clone())))),
                    None => v.insert_entry(StateAccess::Read(None)),
                };
            }
        }
    }
}

impl OrderedRwLog for LeafStateLog {
    fn add_write(&mut self, addr: &Address, value: Account) {
        match self.accounts.entry(*addr) {
            Entry::Occupied(mut existing) => {
                let entry_value = existing.get_mut();
                match entry_value {
                    // If we've already read this slot, turn into into a readThenWrite entry
                    StateAccess::Read(r) => {
                        *entry_value = StateAccess::ReadThenWrite(Box::new(ModifiedAccount {
                            original: r.clone().map(|x| *x),
                            modified: value,
                        }));
                    }
                    // If this slot has already been written, ensure that the value we just read is the one previously written
                    StateAccess::ReadThenWrite(m) => {
                        m.modified = value;
                    }
                    StateAccess::Write(w) => {
                        **w = value;
                    }
                }
            }
            Entry::Vacant(v) => {
                v.insert_entry(StateAccess::Write(Box::new(value)));
            }
        }
    }
}
