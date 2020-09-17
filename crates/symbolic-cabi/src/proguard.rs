#![allow(deprecated)]

use std::ffi::CStr;
use std::os::raw::c_char;
use std::slice;

use symbolic::common::{AsSelf, ByteView, SelfCell};
use symbolic::proguard::ProguardMappingView;

use crate::core::{SymbolicStr, SymbolicUuid};
use crate::utils::ForeignObject;

/// Represents a proguard mapping view.
pub struct SymbolicProguardMappingView;

impl ForeignObject for SymbolicProguardMappingView {
    type RustObject = ProguardMappingView<'static>;
}

ffi_fn! {
    /// Creates a proguard mapping view from a path.
    ///
    /// @deprecated: use `symbolic_proguardmapper_open` instead
    unsafe fn symbolic_proguardmappingview_open(
        path: *const c_char
    ) -> Result<*mut SymbolicProguardMappingView> {
        let byteview = ByteView::open(CStr::from_ptr(path).to_str()?)?;
        let proguard = ProguardMappingView::parse(byteview)?;
        Ok(SymbolicProguardMappingView::from_rust(proguard))
    }
}

ffi_fn! {
    /// Creates a proguard mapping view from bytes.
    ///
    /// This shares the underlying memory and does not copy it.
    ///
    /// @deprecated: use `symbolic_proguardmapper_open` instead
    unsafe fn symbolic_proguardmappingview_from_bytes(
        bytes: *const c_char,
        len: usize
    ) -> Result<*mut SymbolicProguardMappingView> {
        let slice = slice::from_raw_parts(bytes as *const _, len);
        let byteview = ByteView::from_slice(slice);
        let proguard = ProguardMappingView::parse(byteview)?;
        Ok(SymbolicProguardMappingView::from_rust(proguard))
    }
}

ffi_fn! {
    /// Frees a proguard mapping view.
    ///
    /// @deprecated: use `symbolic_proguardmapper_free` instead
    unsafe fn symbolic_proguardmappingview_free(view: *mut SymbolicProguardMappingView) {
        SymbolicProguardMappingView::drop(view);
    }
}

ffi_fn! {
    /// Returns the UUID of a proguard mapping file.
    ///
    /// @deprecated: use `symbolic_proguardmapper_get_uuid` instead
    unsafe fn symbolic_proguardmappingview_get_uuid(
        view: *mut SymbolicProguardMappingView,
    ) -> Result<SymbolicUuid> {
        Ok(SymbolicProguardMappingView::as_rust(view).uuid().into())
    }
}

ffi_fn! {
    /// Converts a dotted path at a line number.
    ///
    /// @deprecated: use `symbolic_proguardmapper_remap_class` or `symbolic_proguardmapper_remap_frame` instead
    unsafe fn symbolic_proguardmappingview_convert_dotted_path(
        view: *const SymbolicProguardMappingView,
        path: *const SymbolicStr,
        lineno: u32,
    ) -> Result<SymbolicStr> {
        let path = (*path).as_str();
        Ok(SymbolicProguardMappingView::as_rust(view).convert_dotted_path(path, lineno).into())
    }
}

ffi_fn! {
    /// Returns true if the mapping file has line infos.
    ///
    /// @deprecated: use `symbolic_proguardmapper_has_line_info` instead
    unsafe fn symbolic_proguardmappingview_has_line_info(
        view: *const SymbolicProguardMappingView,
    ) -> Result<bool> {
        Ok(SymbolicProguardMappingView::as_rust(view).has_line_info())
    }
}

use proguard::{ProguardMapper, ProguardMapping, StackFrame};

/// Represents a Java Stack Frame.
#[repr(C)]
pub struct SymbolicJavaStackFrame {
    pub class_name: SymbolicStr,
    pub method: SymbolicStr,
    pub file: SymbolicStr,
    pub line: usize,
}

/// The result of remapping a Stack Frame.
#[repr(C)]
pub struct SymbolicProguardRemapResult {
    pub frames: *mut SymbolicJavaStackFrame,
    pub len: usize,
}

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

pub struct OwnedProguardMapper<'a> {
    inner: SelfCell<ByteView<'a>, Inner<'a>>,
}

/// Represents a proguard mapper.
pub struct SymbolicProguardMapper;

impl ForeignObject for SymbolicProguardMapper {
    type RustObject = OwnedProguardMapper<'static>;
}

ffi_fn! {
    /// Creates a proguard mapping view from a path.
    unsafe fn symbolic_proguardmapper_open(
        path: *const c_char
    ) -> Result<*mut SymbolicProguardMapper> {
        let byteview = ByteView::open(CStr::from_ptr(path).to_str()?)?;

        let inner = SelfCell::new(byteview, |data| {
            let mapping = ProguardMapping::new(&*data);
            let mapper = ProguardMapper::new(mapping.clone());
            Inner { mapping, mapper }
        });

        let proguard = OwnedProguardMapper { inner };
        Ok(SymbolicProguardMapper::from_rust(proguard))
    }
}

ffi_fn! {
    /// Frees a proguard mapping view.
    unsafe fn symbolic_proguardmapper_free(mapper: *mut SymbolicProguardMapper) {
        SymbolicProguardMapper::drop(mapper);
    }
}

ffi_fn! {
    /// Remaps a Stack Frame.
    unsafe fn symbolic_proguardmapper_remap_frame(
        mapper: *const SymbolicProguardMapper,
        class: *const SymbolicStr,
        method: *const SymbolicStr,
        line: usize,
    ) -> Result<SymbolicProguardRemapResult> {
        let mapper = &SymbolicProguardMapper::as_rust(mapper).inner.get().mapper;
        let frame = StackFrame::new((*class).as_str(), (*method).as_str(), line);

        let mut frames: Vec<_> = mapper.remap_frame(&frame).map(|frame| {
            SymbolicJavaStackFrame {
                class_name: frame.class().to_owned().into(),
                method: frame.method().to_owned().into(),
                file: frame.file().unwrap_or("").to_owned().into(),
                line: frame.line(),
            }
        }).collect();

        frames.shrink_to_fit();
        let rv = SymbolicProguardRemapResult {
            frames: frames.as_mut_ptr(),
            len: frames.len(),
        };
        std::mem::forget(frames);

        Ok(rv)
    }
}

ffi_fn! {
    /// Remaps a class name.
    unsafe fn symbolic_proguardmapper_remap_class(
        mapper: *const SymbolicProguardMapper,
        class: *const SymbolicStr,
    ) -> Result<SymbolicStr> {
        let mapper = &SymbolicProguardMapper::as_rust(mapper).inner.get().mapper;

        let class = (*class).as_str();
        Ok(mapper.remap_class(class).unwrap_or("").to_owned().into())
    }
}

ffi_fn! {
    /// Returns the UUID of a proguard mapping file.
    unsafe fn symbolic_proguardmapper_get_uuid(
        mapper: *mut SymbolicProguardMapper,
    ) -> Result<SymbolicUuid> {
        Ok(SymbolicProguardMapper::as_rust(mapper).inner.get().mapping.uuid().into())
    }
}

ffi_fn! {
    /// Returns true if the mapping file has line infos.
    unsafe fn symbolic_proguardmapper_has_line_info(
        mapper: *const SymbolicProguardMapper,
    ) -> Result<bool> {
        Ok(SymbolicProguardMapper::as_rust(mapper).inner.get().mapping.has_line_info())
    }
}

ffi_fn! {
    /// Frees a remap result.
    unsafe fn symbolic_proguardmapper_result_free(result: *mut SymbolicProguardRemapResult) {
        if !result.is_null() {
            let result = &*result;
            Vec::from_raw_parts(result.frames, result.len, result.len);
        }
    }
}
