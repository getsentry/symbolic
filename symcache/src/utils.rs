/// A quick binary search by key.
pub fn binsearch_by_key<'a, T, B, F>(slice: &'a [T], item: B, mut f: F) -> Option<(usize, &'a T)>
    where B: Ord, F: FnMut(usize, &T) -> B
{
    let mut low = 0;
    let mut high = slice.len();

    while low < high {
        let mid = (low + high) / 2;
        let cur_item = &slice[mid as usize];
        if item < f(mid as usize, cur_item) {
            high = mid;
        } else {
            low = mid + 1;
        }
    }

    if low > 0 && low <= slice.len() {
        Some((low - 1, &slice[low - 1]))
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
    let m = binsearch_by_key(&seq[..], 5, |_, &x| x);
    assert_eq!(*m.unwrap().1, 4);
}
