//! Provides proguard support.
use std::io;
use symbolic_common::byteview::{ByteView, ByteViewHandle};
use uuid::Uuid;

/// A view over a proguard mapping text file.
pub struct ProguardMappingView<'a> {
    mv: ByteViewHandle<'a, proguard::MappingView<'a>>,
}

impl<'a> ProguardMappingView<'a> {
    /// Creates a new proguard mapping view from a byte slice.
    pub fn parse(byteview: ByteView<'a>) -> Result<ProguardMappingView<'a>, io::Error> {
        Ok(ProguardMappingView {
            mv: ByteViewHandle::from_byteview(byteview, |bytes| -> Result<_, io::Error> {
                Ok(proguard::MappingView::from_slice(bytes)?)
            })?,
        })
    }

    /// Returns the mapping UUID.
    pub fn uuid(&self) -> uuid::Uuid {
        // TODO: Remove this once `proguard` has been converted to uuid v0.7.
        Uuid::from_slice(&self.mv.uuid().as_bytes()[..]).unwrap()
    }

    /// Returns true if this file has line infos.
    pub fn has_line_info(&self) -> bool {
        self.mv.has_line_info()
    }

    /// Converts a dotted path.
    pub fn convert_dotted_path(&self, path: &str, lineno: u32) -> String {
        let mut iter = path.splitn(2, ':');
        let cls_name = iter.next().unwrap_or("");
        let meth_name = iter.next();
        if let Some(cls) = self.mv.find_class(cls_name) {
            let class_name = cls.class_name();
            if let Some(meth_name) = meth_name {
                let lineno = if lineno == 0 {
                    None
                } else {
                    Some(lineno as u32)
                };

                let methods = cls.get_methods(meth_name, lineno);
                if !methods.is_empty() {
                    format!("{}:{}", class_name, methods[0].name())
                } else {
                    format!("{}:{}", class_name, meth_name)
                }
            } else {
                class_name.to_string()
            }
        } else {
            path.to_string()
        }
    }
}
