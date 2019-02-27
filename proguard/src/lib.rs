//! Provides proguard support.

#![warn(missing_docs)]

use std::io;

use proguard::MappingView;

use symbolic_common::{AsSelf, ByteView, SelfCell, Uuid};

struct Inner<'a>(MappingView<'a>);

impl<'slf, 'a: 'slf> AsSelf<'slf> for Inner<'a> {
    type Ref = MappingView<'slf>;

    fn as_self(&'slf self) -> &Self::Ref {
        &self.0
    }
}

/// A view over a proguard mapping text file.
pub struct ProguardMappingView<'a> {
    inner: SelfCell<ByteView<'a>, Inner<'a>>,
}

impl<'a> ProguardMappingView<'a> {
    /// Creates a new proguard mapping view from a byte slice.
    pub fn parse(byteview: ByteView<'a>) -> Result<Self, io::Error> {
        // NB: Since ByteView does not expose its inner data structure, we need to use a `SelfCell`
        // to construct a `proguard::MappingView`. Ideally, we would pass the ByteView's backing to
        // the MappingView constructor directly, instead.
        let inner = SelfCell::try_new(byteview, |data| {
            MappingView::from_slice(unsafe { &*data }).map(Inner)
        })?;

        Ok(ProguardMappingView { inner })
    }

    /// Returns the mapping UUID.
    pub fn uuid(&self) -> Uuid {
        self.inner.get().uuid()
    }

    /// Returns true if this file has line infos.
    pub fn has_line_info(&self) -> bool {
        self.inner.get().has_line_info()
    }

    /// Converts a dotted path.
    pub fn convert_dotted_path(&self, path: &str, lineno: u32) -> String {
        let mut iter = path.splitn(2, ':');
        let cls_name = iter.next().unwrap_or("");
        let meth_name = iter.next();
        if let Some(cls) = self.inner.get().find_class(cls_name) {
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
