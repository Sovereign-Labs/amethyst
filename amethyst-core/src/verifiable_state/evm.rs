use std::{
    collections::{hash_map::Entry, HashMap},
    fmt::Debug,
};

use primitive_types::{H256, U256};
use revm::AccountInfo;

use super::{
    Access, EvmAddress, EvmStateEntry, MergeableLog, Modified, OrderedReadLog, OrderedRwLog,
};

#[derive(Default)]
pub struct EvmStateLog {
    pub accounts: HashMap<EvmAddress, Access<AccountInfo>>,
    pub state: HashMap<EvmStorageAddress, Access<U256>>,
    pub blockhashes: HashMap<u64, Option<H256>>,
}

#[derive(Hash, PartialEq, Eq, Debug, Clone)]
pub struct EvmStorageAddress(pub EvmAddress, pub U256);

impl MergeableLog for EvmStateLog {
    type Into = Vec<(EvmAddress, Access<AccountInfo>)>;
    /// Merges the read and write logs of two separate proofs to create a master log with the minimal amount of information
    /// necessary to verify both proofs against the old state commitment and compute the new one.
    /// FIXME: Only merges account read/writes
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

impl MergeableLog for Vec<(EvmAddress, Access<AccountInfo>)> {
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

fn do_add_read<K, V>(map: &mut HashMap<K, Access<V>>, key: K, value: &Option<V>)
where
    K: PartialEq + Eq + Debug + Clone + std::hash::Hash,
    V: Clone + Debug + PartialEq,
{
    match map.entry(key) {
        Entry::Occupied(existing) => {
            match existing.get() {
                // If we've already read this slot, ensure that the two reads match, then discard one.
                Access::Read(r) => {
                    assert_eq!(r.as_ref().map(|x| x.as_ref()), value.as_ref())
                }
                // If this slot has already been written, ensure that the value we just read is the one previously written
                Access::ReadThenWrite(m) => {
                    assert_eq!(m.modified.as_ref().map(|x| x.as_ref()), value.as_ref())
                }
                Access::Write(item) => {
                    assert_eq!(item.as_ref().map(|x| x.as_ref()), value.as_ref())
                }
            };
        }
        Entry::Vacant(vacancy) => {
            match value {
                Some(val) => vacancy.insert_entry(Access::Read(Some(Box::new(val.clone())))),
                None => vacancy.insert_entry(Access::Read(None)),
            };
        }
    }
}

fn do_add_write<K, V>(map: &mut HashMap<K, Access<V>>, key: K, value: Option<V>)
where
    K: PartialEq + Eq + Debug + Clone + std::hash::Hash,
    V: Clone + Debug + PartialEq,
{
    match map.entry(key) {
        Entry::Occupied(mut existing) => {
            let entry_value = existing.get_mut();
            match entry_value {
                // If we've already read this slot, turn into into a readThenWrite entry
                Access::Read(r) => {
                    *entry_value = Access::ReadThenWrite(Modified {
                        original: r.take(),
                        modified: value.map(Box::new),
                    });
                }
                // If this slot has already been written, ensure that the value we just read is the one previously written
                Access::ReadThenWrite(m) => {
                    m.modified = value.map(Box::new);
                }
                Access::Write(w) => {
                    *w = value.map(Box::new);
                }
            }
        }
        Entry::Vacant(v) => {
            v.insert_entry(Access::Write(value.map(Box::new)));
        }
    }
}

impl OrderedReadLog for EvmStateLog {
    type State = EvmStateEntry;

    fn add_read(&mut self, item: &Self::State) {
        match item {
            EvmStateEntry::Accounts(k, v) => do_add_read(&mut self.accounts, *k, v),
            EvmStateEntry::Storage(k, v) => do_add_read(&mut self.state, k.clone(), v),
            EvmStateEntry::Blockhash(blocknumber, hash) => {
                match self.blockhashes.entry(*blocknumber) {
                    Entry::Occupied(e) => assert_eq!(hash, e.get()),
                    Entry::Vacant(vacancy) => {
                        vacancy.insert(*hash);
                    }
                }
            }
        };
    }
}

impl OrderedRwLog for EvmStateLog {
    fn add_write(&mut self, item: Self::State) {
        match item {
            EvmStateEntry::Accounts(k, v) => do_add_write(&mut self.accounts, k, v),
            EvmStateEntry::Storage(k, v) => {
                assert!(v.is_some(), "Storage slots cannot be deleted!");
                do_add_write(&mut self.state, k, v)
            }
            EvmStateEntry::Blockhash(_, _) => unreachable!("block hash cannot be modified"),
        };
    }
}
