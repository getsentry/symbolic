extern crate symbolic_symcache;
extern crate symbolic_debuginfo;
extern crate symbolic_common;

use symbolic_symcache::SymCacheWriter;
use symbolic_debuginfo::FatObject;
use symbolic_common::ByteView;

fn main() {
    let bv = ByteView::from_path("/tmp/88ee46a9-a205-33a8-aa38-7fd10405f318").unwrap();
    let mut out = vec![0u8; 0];
    let mut writer = SymCacheWriter::new(&mut out);
    let fat_obj = FatObject::parse(&bv).unwrap();
    let objects = fat_obj.objects().unwrap();
    writer.write_object(&objects[0]).unwrap();
}
