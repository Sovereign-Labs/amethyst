use std::{
    collections::{hash_map::Entry, HashMap},
    iter::Peekable,
};

use primitive_types::{H160, H256, U256};

type Address = H160;

// #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Account {
    /// Account nonce.
    pub nonce: u64,
    /// Account balance.
    pub balance: u128,
    /// Storage
    pub storage_root: H256,
    /// Account code.
    pub code_hash: H256,
}

#[derive(Clone)]
pub struct ModifiedAccount {
    original: Account,
    modified: Account,
}

#[derive(Clone)]
pub enum StateAccess {
    Read(Box<Account>),
    ReadThenWrite(Box<ModifiedAccount>),
    Write(Box<Account>),
}

impl StateAccess {
    pub fn merge(self, other: Self) -> Self {
        match (self, other) {
            (StateAccess::Read(l), StateAccess::Read(r)) => {
                assert_eq!(*l, *r);
                StateAccess::Read(l)
            }
            (StateAccess::Read(l), StateAccess::ReadThenWrite(r)) => {
                assert_eq!(*l, r.original);
                StateAccess::ReadThenWrite(r)
            }
            (StateAccess::Read(l), StateAccess::Write(r)) => {
                StateAccess::ReadThenWrite(Box::new(ModifiedAccount {
                    original: *l,
                    modified: *r,
                }))
            }
            (StateAccess::ReadThenWrite(l), StateAccess::Read(r)) => {
                assert_eq!(l.modified, *r);
                StateAccess::ReadThenWrite(l)
            }
            (StateAccess::ReadThenWrite(mut l), StateAccess::ReadThenWrite(r)) => {
                assert_eq!(l.modified, r.original);
                l.modified = r.modified;
                StateAccess::ReadThenWrite(l)
            }
            (StateAccess::ReadThenWrite(mut l), StateAccess::Write(r)) => {
                l.modified = *r;
                StateAccess::ReadThenWrite(l)
            }
            (StateAccess::Write(l), StateAccess::Read(r)) => {
                assert_eq!(*l, *r);
                StateAccess::Write(l)
            }
            (StateAccess::Write(l), StateAccess::ReadThenWrite(r)) => {
                assert_eq!(*l, r.original);
                StateAccess::Write(Box::new(r.modified))
            }
            (StateAccess::Write(_), StateAccess::Write(r)) => StateAccess::Write(r),
        }
    }
}

pub struct LeafStateLog {
    pub accounts: HashMap<Address, StateAccess>,
    pub state: HashMap<(Address, U256), U256>,
}

pub enum EitherOrBoth<T, U> {
    Both(T, U),
    Left(T),
    Right(U),
}

pub struct UnequalZipper<T, U>
where
    T: Iterator,
    U: Iterator,
{
    l: T,
    r: U,
}
pub trait ZipHelper {
    fn zip_all<O: IntoIterator<IntoIter = U>, T: Iterator, U: Iterator>(
        self,
        other: O,
    ) -> UnequalZipper<T, U>
    where
        Self: IntoIterator<IntoIter = T> + Sized,
    {
        UnequalZipper {
            l: self.into_iter(),
            r: other.into_iter(),
        }
    }
}

// Take two size hints in the format (lower_bound, Option<upper_bound>) and construct a size hint for the longer of the two.
// Per the size_hint documentation, returns `None` for the upper bound if either constituent returns an upper bound of `None`.
fn size_hint_max(l: (usize, Option<usize>), r: (usize, Option<usize>)) -> (usize, Option<usize>) {
    let lower_bound = std::cmp::max(l.0, r.0);
    let upper_bound = match (l.1, r.1) {
        (Some(left_upper), Some(right_upper)) => Some(std::cmp::max(left_upper, right_upper)),
        _ => None,
    };
    (lower_bound, upper_bound)
}

impl<T, U, L, R> std::iter::Iterator for UnequalZipper<T, U>
where
    T: Iterator<Item = L>,
    U: Iterator<Item = R>,
{
    type Item = EitherOrBoth<L, R>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match (self.l.next(), self.r.next()) {
            (None, None) => None,
            (Some(l), None) => Some(EitherOrBoth::Left(l)),
            (None, Some(r)) => Some(EitherOrBoth::Right(r)),
            (Some(l), Some(r)) => Some(EitherOrBoth::Both(l, r)),
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        size_hint_max(self.l.size_hint(), self.r.size_hint())
    }
}

pub trait MergeableLog {
    type Into: MergeableLog;
    /// Merge two different state-access logs into a single one
    fn merge(self, rhs: Self) -> Self::Into;
}

pub trait OrderedReadLog: MergeableLog {
    fn add_read(&mut self, addr: &Address, value: Account);
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
    fn add_read(&mut self, addr: &Address, value: Account) {
        match self.accounts.entry(*addr) {
            Entry::Occupied(existing) => {
                match existing.get() {
                    // If we've already read this slot, ensure that the two reads match, then discard one.
                    StateAccess::Read(r) => {
                        assert_eq!(**r, value)
                    }
                    // If this slot has already been written, ensure that the value we just read is the one previously written
                    StateAccess::ReadThenWrite(m) => {
                        assert_eq!(m.modified, value)
                    }
                    StateAccess::Write(w) => {
                        assert_eq!(**w, value)
                    }
                };
            }
            Entry::Vacant(v) => {
                v.insert_entry(StateAccess::Read(Box::new(value)));
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
                            original: *r.clone(),
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

pub struct Merger<T, U, X>
where
    T: Iterator<Item = X>,
    U: Iterator<Item = X>,
{
    l: Peekable<T>,
    r: Peekable<U>,
}

// pub trait MergeIter {
//     fn merge<
//         Rhs: IntoIterator<IntoIter = R>,
//         L: Iterator<Item = X>,
//         R: Iterator<Item = X>,
//         X: PartialOrd,
//     >(
//         self,
//         rhs: Rhs,
//     ) -> Merger<L, R, X>
//     where
//         Self: IntoIterator<IntoIter = L> + Sized,
//     {
//         Merger {
//             l: self.into_iter().peekable(),
//             r: rhs.into_iter().peekable(),
//         }
//     }
// }

pub trait MergeIter<Rhs, L, R, X>
where
    Self: IntoIterator<IntoIter = L> + Sized,
    Rhs: IntoIterator<IntoIter = R>,
    L: Iterator<Item = X>,
    R: Iterator<Item = X>,
    X: PartialOrd,
{
    fn merge(self, rhs: Rhs) -> Merger<L, R, X> {
        Merger {
            l: self.into_iter().peekable(),
            r: rhs.into_iter().peekable(),
        }
    }
}

impl<Lhs, Rhs, L, R, X> MergeIter<Rhs, L, R, X> for Lhs
where
    Lhs: IntoIterator<IntoIter = L>,
    Rhs: IntoIterator<IntoIter = R>,
    L: Iterator<Item = X>,
    R: Iterator<Item = X>,
    X: PartialOrd,
{
}

impl<L: Iterator<Item = X>, R: Iterator<Item = X>, X: PartialOrd> Iterator for Merger<L, R, X> {
    type Item = X;

    fn next(&mut self) -> Option<Self::Item> {
        if self.r.peek() > self.l.peek() {
            self.r.next()
        } else {
            self.l.next()
        }
    }
}
