use pyo3::prelude::*;
use symbolic::common::{AsSelf, ByteView, SelfCell, Uuid};

#[pyclass(frozen)]
pub struct JavaStackFrame {
    #[pyo3(get)]
    pub class_name: String,
    #[pyo3(get)]
    pub method: String,
    #[pyo3(get)]
    pub file: Option<String>,
    #[pyo3(get)]
    pub line: usize,
}

struct Inner<'a> {
    mapping: proguard::ProguardMapping<'a>,
    mapper: proguard::ProguardMapper<'a>,
}

impl<'slf, 'a: 'slf> AsSelf<'slf> for Inner<'a> {
    type Ref = Inner<'slf>;

    fn as_self(&'slf self) -> &Self::Ref {
        self
    }
}

#[pyclass(frozen)]
pub struct ProguardMapper {
    inner: SelfCell<ByteView<'static>, Inner<'static>>,
}

#[pymethods]
impl ProguardMapper {
    #[staticmethod]
    pub fn open(path: &str) -> PyResult<Self> {
        let byteview = ByteView::open(path)?;

        let inner = SelfCell::new(byteview, |data| {
            let mapping = proguard::ProguardMapping::new(unsafe { &*data });
            let mapper = proguard::ProguardMapper::new(mapping.clone());
            Inner { mapping, mapper }
        });

        Ok(ProguardMapper { inner })
    }

    /// Returns the UUID of the file.
    /* FIXME:
    #[getter]
    pub fn uuid(&self) -> Uuid {
        self.inner.get().mapping.uuid()
    }*/

    /// True if the file contains line information.
    #[getter]
    pub fn has_line_info(&self) -> bool {
        self.inner.get().mapping.has_line_info()
    }

    /// Remaps the given class name.
    pub fn remap_class(&self, klass: &str) -> Option<&str> {
        self.inner.get().mapper.remap_class(klass)
    }

    /// Remaps the given class and method name if that can be done unambiguously.
    pub fn remap_method(&self, klass: &str, method: &str) -> Option<(&str, &str)> {
        self.inner.get().mapper.remap_method(klass, method)
    }

    pub fn remap_frame(&self, klass: &str, method: &str, line: usize) -> Vec<JavaStackFrame> {
        let frame = proguard::StackFrame::new(klass, method, line);
        self.inner
            .get()
            .mapper
            .remap_frame(&frame)
            .map(|frame| JavaStackFrame {
                class_name: frame.class().into(),
                method: frame.method().into(),
                file: frame.file().map(Into::into),
                line: frame.line(),
            })
            .collect()
    }
}
