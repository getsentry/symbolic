extern crate symbolic_symcache;
extern crate symbolic_debuginfo;
extern crate symbolic_common;

use symbolic_symcache::SymCache;
use symbolic_debuginfo::FatObject;
use symbolic_common::ByteView;

fn main() {
    let bv = ByteView::from_path("/Users/mitsuhiko/Downloads/hello/hello.dSYM/Contents/Resources/DWARF/hello").unwrap();
    //let bv = ByteView::from_path("/tmp/88ee46a9-a205-33a8-aa38-7fd10405f318").unwrap();
    let fat_obj = FatObject::parse(bv).unwrap();
    let objects = fat_obj.objects().unwrap();
    let cache = SymCache::from_object(&objects[0]).unwrap();
}
