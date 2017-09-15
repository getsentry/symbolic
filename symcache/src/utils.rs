use std::hash::Hash;
use std::borrow::Borrow;
use std::collections::HashMap;

use num::{Integer, Unsigned};

pub struct IdMap<K, I> {
    map: HashMap<K, I>,
    counter: I,
}

impl<K, I> IdMap<K, I>
    where K: Eq + Hash,
          I: Copy + Integer + Unsigned,
{
    pub fn new() -> IdMap<K, I> {
        IdMap {
            map: HashMap::new(),
            counter: I::zero(),
        }
    }

    pub fn get_id<Q>(&mut self, k: &Q) -> I
        where K: Borrow<Q>,
              Q: ?Sized + Hash + Eq + ToOwned<Owned=K>,
    {
        if let Some(idx) = self.map.get(k) {
            return idx.to_owned();
        }

        let idx = self.counter;
        self.map.insert(k.to_owned(), idx);
        self.counter = idx + I::one();
        idx
    }

    pub fn into_hashmap(self) -> HashMap<K, I> {
        self.map
    }
}

/// A quick binary search by key.
pub fn binsearch_by_key<'a, T, B, F>(slice: &'a [T], item: B, mut f: F) -> Option<&'a T>
    where B: Ord, F: FnMut(&T) -> B
{
    let mut low = 0;
    let mut high = slice.len();

    while low < high {
        let mid = (low + high) / 2;
        let cur_item = &slice[mid as usize];
        if item < f(cur_item) {
            high = mid;
        } else {
            low = mid + 1;
        }
    }

    if low > 0 && low <= slice.len() {
        Some(&slice[low - 1])
    } else {
        None
    }
}

#[test]
fn test_idmap() {
    let mut m: IdMap<String, u8> = IdMap::new();
    assert_eq!(m.get_id("foo"), 0u8);
    assert_eq!(m.get_id("bar"), 1u8);
    assert_eq!(m.get_id("bar"), 1u8);
    assert_eq!(m.get_id("foo"), 0u8);
}

#[test]
fn test_binsearch() {
    let seq = [0u32, 2, 4, 6, 8, 10];
    let m = binsearch_by_key(&seq[..], 5, |&x| x);
    assert_eq!(*m.unwrap(), 4);
}
