pub mod convert;
pub mod lookup;
pub mod raw;

use crate::{ErrorKind, Result};

use symbolic_common::Arch;
use watto::{Pod, StringTable, align_to};

/// The serialized SymCache V9 binary format.
#[derive(Clone)]
pub struct SymCache<'data> {
    pub header: &'data raw::Header,
    pub files: &'data [raw::File],
    pub functions: &'data [raw::Function],
    pub source_locations: &'data [raw::SourceLocation],
    pub ranges: &'data [raw::Range],
    pub string_bytes: &'data [u8],
    pub variable_header: Option<&'data raw::VariableHeader>,
    pub function_variables: &'data [raw::FunctionVariables],
    pub variables: &'data [raw::Variable],
    pub variable_locations: &'data [raw::VariableLocationInfo],
    pub types: &'data [raw::Type],
}

impl std::fmt::Debug for SymCache<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut d = f.debug_struct("SymCache");
        d.field("version", &9)
            .field("debug_id", &self.header.debug_id)
            .field("arch", &Arch::from_u32(self.header.arch))
            .field("files", &self.header.num_files)
            .field("functions", &self.header.num_functions)
            .field("source_locations", &self.header.num_source_locations)
            .field("ranges", &self.header.num_ranges)
            .field("string_bytes", &self.header.string_bytes);

        if let Some(vh) = self.variable_header {
            d.field("variables", &vh.num_variables)
                .field("variable_locations", &vh.num_variable_locations)
                .field("types", &vh.num_types);
        }

        d.finish()
    }
}

impl<'data> SymCache<'data> {
    pub fn parse(buf: &'data [u8]) -> Result<Self> {
        let (header, rest) = raw::Header::ref_from_prefix(buf).ok_or(ErrorKind::InvalidHeader)?;

        let (_, rest) = align_to(rest, 8).ok_or(ErrorKind::InvalidFiles)?;
        let (files, rest) = raw::File::slice_from_prefix(rest, header.num_files as usize)
            .ok_or(ErrorKind::InvalidFiles)?;

        let (_, rest) = align_to(rest, 8).ok_or(ErrorKind::InvalidFunctions)?;
        let (functions, rest) =
            raw::Function::slice_from_prefix(rest, header.num_functions as usize)
                .ok_or(ErrorKind::InvalidFunctions)?;

        let (_, rest) = align_to(rest, 8).ok_or(ErrorKind::InvalidSourceLocations)?;
        let (source_locations, rest) =
            raw::SourceLocation::slice_from_prefix(rest, header.num_source_locations as usize)
                .ok_or(ErrorKind::InvalidSourceLocations)?;

        let (_, rest) = align_to(rest, 8).ok_or(ErrorKind::InvalidRanges)?;
        let (ranges, rest) = raw::Range::slice_from_prefix(rest, header.num_ranges as usize)
            .ok_or(ErrorKind::InvalidRanges)?;

        let (_, rest) = align_to(rest, 8).ok_or(ErrorKind::UnexpectedStringBytes {
            expected: header.string_bytes as usize,
            found: 0,
        })?;

        let (string_bytes, rest) = rest.split_at_checked(header.string_bytes as usize).ok_or(
            ErrorKind::UnexpectedStringBytes {
                expected: header.string_bytes as usize,
                found: rest.len(),
            },
        )?;

        let mut variable_header = None;
        let mut function_variables = &[][..];
        let mut variables = &[][..];
        let mut variable_locations = &[][..];
        let mut types = &[][..];

        if header.variable_header == raw::VARIABLE_HEADER_VERSION {
            let (_, rest) = align_to(rest, 8).ok_or(ErrorKind::InvalidVariableHeader)?;
            let (vh, rest) = raw::VariableHeader::ref_from_prefix(rest)
                .ok_or(ErrorKind::InvalidVariableHeader)?;
            variable_header = Some(vh);

            let (_, rest) = align_to(rest, 8).ok_or(ErrorKind::InvalidFunctionVariables)?;
            let (fv, rest) =
                raw::FunctionVariables::slice_from_prefix(rest, header.num_functions as usize)
                    .ok_or(ErrorKind::InvalidFunctionVariables)?;
            function_variables = fv;

            let (_, rest) = align_to(rest, 8).ok_or(ErrorKind::InvalidVariables)?;
            let (vars, rest) = raw::Variable::slice_from_prefix(rest, vh.num_variables as usize)
                .ok_or(ErrorKind::InvalidVariables)?;
            variables = vars;

            let (_, rest) = align_to(rest, 8).ok_or(ErrorKind::InvalidVariableLocations)?;
            let (vl, rest) = raw::VariableLocationInfo::slice_from_prefix(
                rest,
                vh.num_variable_locations as usize,
            )
            .ok_or(ErrorKind::InvalidVariableLocations)?;
            variable_locations = vl;

            let (_, rest) = align_to(rest, 8).ok_or(ErrorKind::InvalidTypes)?;
            let (tys, _) = raw::Type::slice_from_prefix(rest, vh.num_types as usize)
                .ok_or(ErrorKind::InvalidTypes)?;
            types = tys;
        }

        Ok(Self {
            header,
            files,
            functions,
            source_locations,
            ranges,
            string_bytes,
            variable_header,
            function_variables,
            variables,
            variable_locations,
            types,
        })
    }

    /// Resolves a string reference to the pointed-to `&str` data.
    fn get_string(&self, offset: u32) -> Option<&'data str> {
        // version >= 8 (including v9): string length prefixes are LEB128
        StringTable::read(self.string_bytes, offset as usize).ok()
    }
}
