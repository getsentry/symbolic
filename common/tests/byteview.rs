extern crate symbolic_common;

use std::fs;
use std::env;

use symbolic_common::ByteView;


#[test]
fn test_basics() {
    let mut path = env::temp_dir();
    path.push(".c0b41a59-801b-4d18-aaa1-88432736116d.empty");
    {
        fs::File::create(&path).unwrap();
    }
    let bv = ByteView::from_path(&path).unwrap();
    assert_eq!(&bv[..], &b""[..]);
    fs::remove_file(&path).unwrap();
}
