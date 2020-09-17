//! Provides proguard support.

#![warn(missing_docs)]

use std::io;

use proguard::{ProguardMapper, ProguardMapping, StackFrame};

use symbolic_common::{AsSelf, ByteView, SelfCell, Uuid};

struct Inner<'a> {
    mapping: ProguardMapping<'a>,
    mapper: ProguardMapper<'a>,
}

impl<'slf, 'a: 'slf> AsSelf<'slf> for Inner<'a> {
    type Ref = Inner<'slf>;

    fn as_self(&'slf self) -> &Self::Ref {
        &self
    }
}

/// A view over a proguard mapping text file.
#[deprecated = "use the `proguard` crate directly"]
pub struct ProguardMappingView<'a> {
    inner: SelfCell<ByteView<'a>, Inner<'a>>,
}

#[allow(deprecated)]
impl<'a> ProguardMappingView<'a> {
    /// Creates a new proguard mapping view from a byte slice.
    pub fn parse(byteview: ByteView<'a>) -> Result<Self, io::Error> {
        let inner = SelfCell::new(byteview, |data| {
            let mapping = ProguardMapping::new(unsafe { &*data });
            let mapper = ProguardMapper::new(mapping.clone());
            Inner { mapping, mapper }
        });

        Ok(ProguardMappingView { inner })
    }

    /// Returns the mapping UUID.
    pub fn uuid(&self) -> Uuid {
        self.inner.get().mapping.uuid()
    }

    /// Returns true if this file has line infos.
    pub fn has_line_info(&self) -> bool {
        self.inner.get().mapping.has_line_info()
    }

    /// Converts a dotted path.
    pub fn convert_dotted_path(&self, path: &str, lineno: u32) -> String {
        let mapper = &self.inner.get().mapper;

        let mut iter = path.splitn(2, ':');
        let cls_name = iter.next().unwrap_or("");
        match iter.next() {
            Some(meth_name) => {
                let mut mapped =
                    mapper.remap_frame(&StackFrame::new(cls_name, meth_name, lineno as usize));
                match mapped.next() {
                    Some(frame) => format!("{}:{}", frame.class(), frame.method()),
                    None => path.to_string(),
                }
            }
            None => mapper.remap_class(cls_name).unwrap_or(cls_name).to_string(),
        }
    }
}
