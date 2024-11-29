use std::fmt;
use std::ops::{Index, IndexMut};

use symbolic_common::Uuid;
use watto::Pod;

use super::{FormatError, FormatErrorKind, PortablePdb};

/// An enumeration of all table types in ECMA-335 and Portable PDB.
#[repr(usize)]
#[derive(Debug, Clone, Copy)]
pub enum TableType {
    Assembly = 0x20,
    AssemblyOs = 0x22,
    AssemblyProcessor = 0x21,
    AssemblyRef = 0x23,
    AssemblyRefOs = 0x25,
    AssemblyRefProcessor = 0x24,
    ClassLayout = 0x0F,
    Constant = 0x0B,
    CustomAttribute = 0x0C,
    DeclSecurity = 0x0E,
    EventMap = 0x12,
    Event = 0x14,
    ExportedType = 0x27,
    Field = 0x04,
    FieldLayout = 0x10,
    FieldMarshal = 0x0D,
    FieldRVA = 0x1D,
    File = 0x26,
    GenericParam = 0x2A,
    GenericParamConstraint = 0x2C,
    ImplMap = 0x1C,
    InterfaceImpl = 0x09,
    ManifestResource = 0x28,
    MemberRef = 0x0A,
    MethodDef = 0x06,
    MethodImpl = 0x19,
    MethodSemantics = 0x18,
    MethodSpec = 0x2B,
    Module = 0x00,
    ModuleRef = 0x1A,
    NestedClass = 0x29,
    Param = 0x08,
    Property = 0x17,
    PropertyMap = 0x15,
    StandAloneSig = 0x11,
    TypeDef = 0x02,
    TypeRef = 0x01,
    TypeSpec = 0x1B,
    // portable pdb extension starts here
    CustomDebugInformation = 0x37,
    Document = 0x30,
    ImportScope = 0x35,
    LocalConstant = 0x34,
    LocalScope = 0x32,
    LocalVariable = 0x33,
    MethodDebugInformation = 0x31,
    StateMachineMethod = 0x36,
    DummyEmpty = 0x3F,
}

impl From<usize> for TableType {
    fn from(value: usize) -> Self {
        match value {
            x if x == Self::Assembly as usize => Self::Assembly,
            x if x == Self::AssemblyProcessor as usize => Self::AssemblyProcessor,
            x if x == Self::AssemblyRef as usize => Self::AssemblyRef,
            x if x == Self::AssemblyRefOs as usize => Self::AssemblyRefOs,
            x if x == Self::AssemblyRefProcessor as usize => Self::AssemblyRefProcessor,
            x if x == Self::ClassLayout as usize => Self::ClassLayout,
            x if x == Self::Constant as usize => Self::Constant,
            x if x == Self::CustomAttribute as usize => Self::CustomAttribute,
            x if x == Self::DeclSecurity as usize => Self::DeclSecurity,
            x if x == Self::EventMap as usize => Self::EventMap,
            x if x == Self::Event as usize => Self::Event,
            x if x == Self::ExportedType as usize => Self::ExportedType,
            x if x == Self::Field as usize => Self::Field,
            x if x == Self::FieldLayout as usize => Self::FieldLayout,
            x if x == Self::FieldMarshal as usize => Self::FieldMarshal,
            x if x == Self::FieldRVA as usize => Self::FieldRVA,
            x if x == Self::File as usize => Self::File,
            x if x == Self::GenericParam as usize => Self::GenericParam,
            x if x == Self::GenericParamConstraint as usize => Self::GenericParamConstraint,
            x if x == Self::ImplMap as usize => Self::ImplMap,
            x if x == Self::InterfaceImpl as usize => Self::InterfaceImpl,
            x if x == Self::ManifestResource as usize => Self::ManifestResource,
            x if x == Self::MemberRef as usize => Self::MemberRef,
            x if x == Self::MethodDef as usize => Self::MethodDef,
            x if x == Self::MethodImpl as usize => Self::MethodImpl,
            x if x == Self::MethodSemantics as usize => Self::MethodSemantics,
            x if x == Self::MethodSpec as usize => Self::MethodSpec,
            x if x == Self::Module as usize => Self::Module,
            x if x == Self::ModuleRef as usize => Self::ModuleRef,
            x if x == Self::NestedClass as usize => Self::NestedClass,
            x if x == Self::Param as usize => Self::Param,
            x if x == Self::Property as usize => Self::Property,
            x if x == Self::PropertyMap as usize => Self::PropertyMap,
            x if x == Self::StandAloneSig as usize => Self::StandAloneSig,
            x if x == Self::TypeDef as usize => Self::TypeDef,
            x if x == Self::TypeRef as usize => Self::TypeRef,
            x if x == Self::TypeSpec as usize => Self::TypeSpec,
            x if x == Self::CustomDebugInformation as usize => Self::CustomDebugInformation,
            x if x == Self::Document as usize => Self::Document,
            x if x == Self::ImportScope as usize => Self::ImportScope,
            x if x == Self::LocalConstant as usize => Self::LocalConstant,
            x if x == Self::LocalScope as usize => Self::LocalScope,
            x if x == Self::LocalVariable as usize => Self::LocalVariable,
            x if x == Self::MethodDebugInformation as usize => Self::MethodDebugInformation,
            x if x == Self::StateMachineMethod as usize => Self::StateMachineMethod,
            _ => Self::DummyEmpty,
        }
    }
}

/// A table in a Portable PDB file.
#[derive(Clone, Copy)]
pub struct Table<'data> {
    type_: TableType,
    /// The number of rows in the table.
    pub rows: usize,
    /// The width in bytes of one table row.
    width: usize,
    columns: [Column; 6],
    /// The bytes covered by the table.
    ///
    /// The length of `contents` should be equal to `rows * width`.
    contents: &'data [u8],
}

impl fmt::Debug for Table<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cols: Vec<usize> = self
            .columns
            .iter()
            .map(|c| c.width)
            .take_while(|w| *w > 0)
            .collect();
        let mut rows = Vec::new();
        let mut bytes = self.contents;
        for _ in 0..self.rows {
            let (mut row_bytes, rest) = bytes.split_at(self.width);
            bytes = rest;
            let mut row = Vec::new();
            for col_width in cols.iter() {
                let (col_bytes, rest) = row_bytes.split_at(*col_width);
                row_bytes = rest;
                row.push(col_bytes);
            }
            rows.push(row);
        }
        f.debug_struct("Table")
            .field("schema", &cols)
            .field("rows", &self.rows)
            .field("contents", &rows)
            .finish()
    }
}

impl<'data> Table<'data> {
    /// Sets the tables column widths to the specified values.
    ///
    /// Also sets [`width`](Table::width) to the sum of the provided column widths.
    fn set_columns(
        &mut self,
        width0: usize,
        width1: usize,
        width2: usize,
        width3: usize,
        width4: usize,
        width5: usize,
    ) {
        self.width = width0 + width1 + width2 + width3 + width4 + width5;

        self.columns[0].offset = 0;
        self.columns[0].width = width0;

        if width1 != 0 {
            self.columns[1].offset = self.columns[0].end();
            self.columns[1].width = width1;
        }

        if width2 != 0 {
            self.columns[2].offset = self.columns[1].end();
            self.columns[2].width = width2;
        }

        if width3 != 0 {
            self.columns[3].offset = self.columns[2].end();
            self.columns[3].width = width3;
        }

        if width4 != 0 {
            self.columns[4].offset = self.columns[3].end();
            self.columns[4].width = width4;
        }

        if width5 != 0 {
            self.columns[5].offset = self.columns[4].end();
            self.columns[5].width = width5;
        }
    }

    /// Sets this table's contents to the first `rows * width` bytes of the provided slice.
    ///
    /// # Panics
    /// Panics if `buf` is not long enough.
    fn set_contents(&mut self, buf: &mut &'data [u8]) {
        if self.rows > 0 {
            let (contents, rest) = buf.split_at(self.rows * self.width);
            self.contents = contents;
            *buf = rest
        }
    }

    /// Returns the the bytes of the `idx`th row, if any.
    ///
    /// Note that table row indices are 1-based!
    pub(crate) fn get_row(&self, idx: usize) -> Result<Row, FormatError> {
        idx.checked_sub(1)
            .and_then(|idx| self.contents.get(idx * self.width..(idx + 1) * self.width))
            .map(|data| Row { data, table: self })
            .ok_or_else(|| FormatErrorKind::RowIndexOutOfBounds(self.type_, idx).into())
    }
}

/// A row in a [Table].
#[derive(Debug, Clone, Copy)]
pub(crate) struct Row<'data> {
    data: &'data [u8],
    table: &'data Table<'data>,
}

impl Row<'_> {
    /// Reads the `col` cell in the given table as a `u32`.
    ///
    /// This returns an error if the indices are out of bounds for the table
    /// or the cell is too wide for a `u32`.
    ///
    /// Note that row and column indices are 1-based!
    pub(crate) fn get_col_u32(&self, col: usize) -> Result<u32, FormatError> {
        if !(1..=6).contains(&col) {
            return Err(FormatErrorKind::ColIndexOutOfBounds(self.table.type_, col).into());
        }
        let Column { offset, width } = self.table.columns[col - 1];
        match width {
            1 => Ok(self.data[offset] as u32),
            2 => {
                let bytes = &self.data[offset..offset + 2];
                Ok(u16::from_ne_bytes(bytes.try_into().unwrap()) as u32)
            }
            4 => {
                let bytes = &self.data[offset..offset + 4];
                Ok(u32::from_ne_bytes(bytes.try_into().unwrap()))
            }

            _ => Err(FormatErrorKind::ColumnWidth(self.table.type_, col, width).into()),
        }
    }
}

/// A column in a [Table].
#[derive(Debug, Default, Clone, Copy)]
struct Column {
    /// The number of bytes from the start of the row to the start of the column.0
    offset: usize,
    /// The width of the column in bytes.
    width: usize,
}

impl Column {
    /// The number of the first byte past the column.
    fn end(self) -> usize {
        self.offset + self.width
    }
}

/// A collection of the sizes of various indices needed for the calculation of table sizes.
///
/// There are three types of indices recorded here:
/// * Heap indices (`string_heap`, `guid_heap`, `blob_heap`) are indices into other sections
///   of the Portable PDB file. Their sizes are determined by the `heap_sizes` bitvector in the #~ stream
///   header, see <https://github.com/stakx/ecma-335/blob/master/docs/ii.24.2.6-metadata-stream.md>.
/// * Table indices (`<something>_table`) are indices into individual tables in this stream. Their sizes
///   are determined by the number of rows in the target table.
/// * Composite indices are indices that may point into one of several tables in this stream. Their sizes are
///   determined both by the number of target tables and the maximum number of rows among them.
#[derive(Debug, Clone)]
struct IndexSizes {
    /// Indices into blobs
    string_heap: usize,
    guid_heap: usize,
    blob_heap: usize,

    /// ECMA-335 table indices
    assembly_ref_table: usize,
    event_table: usize,
    field_table: usize,
    generic_param_table: usize,
    method_def_table: usize,
    module_ref_table: usize,
    param_table: usize,
    property_table: usize,
    type_def_table: usize,

    /// Portable PDB table indices
    document_table: usize,
    import_scope_table: usize,
    local_constant_table: usize,
    local_variable_table: usize,

    /// ECMA-335 composite indices
    type_def_or_ref: usize,
    has_constant: usize,
    has_custom_attribute: usize,
    has_field_marshal: usize,
    has_decl_security: usize,
    member_ref_parent: usize,
    has_semantics: usize,
    method_def_or_ref: usize,
    member_forwarded: usize,
    implementation: usize,
    custom_attribute_type: usize,
    resolution_scope: usize,
    type_or_method_def: usize,

    /// Portable PDB composite indices
    has_custom_debug_information: usize,
}

/// A stream representing the "metadata heap", which comprises a number of metadata tables.
///
/// See <https://github.com/stakx/ecma-335/blob/master/docs/ii.24.2.6-metadata-stream.md> for a definition
/// of the stream's format. Note that this stream contains all tables described in the ECMA-335 specification and
/// the Portable PDB specification.
#[derive(Debug, Clone)]
pub struct MetadataStream<'data> {
    header: &'data super::raw::MetadataStreamHeader,
    tables: [Table<'data>; 64],
}

impl<'data> MetadataStream<'data> {
    pub fn parse(buf: &'data [u8], referenced_table_sizes: [u32; 64]) -> Result<Self, FormatError> {
        let (header, mut rest) = super::raw::MetadataStreamHeader::ref_from_prefix(buf)
            .ok_or(FormatErrorKind::InvalidHeader)?;

        // TODO: verify major/minor version
        // TODO: verify reserved
        let mut tables = [Table {
            type_: TableType::DummyEmpty,
            rows: usize::default(),
            width: usize::default(),
            columns: [Column::default(); 6],
            contents: <&[u8]>::default(),
        }; 64];
        for (i, table) in tables.iter_mut().enumerate() {
            if (header.valid_tables >> i & 1) == 0 {
                continue;
            }

            let (len, rest_) = u32::ref_from_prefix(rest).ok_or(FormatErrorKind::InvalidLength)?;
            rest = rest_;
            table.type_ = TableType::from(i);
            table.rows = *len as usize;
        }

        let table_contents = rest;
        let mut result = Self { header, tables };

        result.set_columns(&referenced_table_sizes);

        let total_length: usize = result
            .tables
            .iter()
            .map(|table| table.width * table.rows)
            .sum();
        if total_length > table_contents.len() {
            return Err(
                FormatErrorKind::InsufficientTableData(total_length, table_contents.len()).into(),
            );
        }

        result.set_contents(table_contents);

        Ok(result)
    }

    /// Sets the column widths of all tables in this stream.
    fn set_columns(&mut self, referenced_table_sizes: &[u32; 64]) {
        use TableType::*;

        let index_sizes = self.index_sizes(referenced_table_sizes);

        self[Assembly].set_columns(
            4,
            8,
            4,
            index_sizes.blob_heap,
            index_sizes.string_heap,
            index_sizes.string_heap,
        );
        self[AssemblyOs].set_columns(4, 4, 4, 0, 0, 0);
        self[AssemblyProcessor].set_columns(4, 0, 0, 0, 0, 0);
        self[AssemblyRef].set_columns(
            8,
            4,
            index_sizes.blob_heap,
            index_sizes.string_heap,
            index_sizes.string_heap,
            index_sizes.blob_heap,
        );
        self[AssemblyRefOs].set_columns(4, 4, 4, index_sizes.assembly_ref_table, 0, 0);
        self[AssemblyRefProcessor].set_columns(4, index_sizes.assembly_ref_table, 0, 0, 0, 0);
        self[ClassLayout].set_columns(2, 4, index_sizes.type_def_table, 0, 0, 0);
        self[Constant].set_columns(2, index_sizes.has_constant, index_sizes.blob_heap, 0, 0, 0);
        self[CustomAttribute].set_columns(
            index_sizes.has_custom_attribute,
            index_sizes.custom_attribute_type,
            index_sizes.blob_heap,
            0,
            0,
            0,
        );
        self[DeclSecurity].set_columns(
            2,
            index_sizes.has_decl_security,
            index_sizes.blob_heap,
            0,
            0,
            0,
        );
        self[EventMap].set_columns(
            index_sizes.type_def_table,
            index_sizes.event_table,
            0,
            0,
            0,
            0,
        );
        self[Event].set_columns(
            2,
            index_sizes.string_heap,
            index_sizes.type_def_or_ref,
            0,
            0,
            0,
        );
        self[ExportedType].set_columns(
            4,
            4,
            index_sizes.string_heap,
            index_sizes.string_heap,
            index_sizes.implementation,
            0,
        );
        self[Field].set_columns(2, index_sizes.string_heap, index_sizes.blob_heap, 0, 0, 0);
        self[FieldLayout].set_columns(4, index_sizes.field_table, 0, 0, 0, 0);
        self[FieldMarshal].set_columns(
            index_sizes.has_field_marshal,
            index_sizes.blob_heap,
            0,
            0,
            0,
            0,
        );
        self[FieldRVA].set_columns(4, index_sizes.field_table, 0, 0, 0, 0);
        self[File].set_columns(4, index_sizes.string_heap, index_sizes.blob_heap, 0, 0, 0);
        self[GenericParam].set_columns(
            2,
            2,
            index_sizes.type_or_method_def,
            index_sizes.string_heap,
            0,
            0,
        );
        self[GenericParamConstraint].set_columns(
            index_sizes.generic_param_table,
            index_sizes.type_def_or_ref,
            0,
            0,
            0,
            0,
        );
        self[ImplMap].set_columns(
            2,
            index_sizes.member_forwarded,
            index_sizes.string_heap,
            index_sizes.module_ref_table,
            0,
            0,
        );
        self[InterfaceImpl].set_columns(
            index_sizes.type_def_table,
            index_sizes.type_def_or_ref,
            0,
            0,
            0,
            0,
        );
        self[ManifestResource].set_columns(
            4,
            4,
            index_sizes.string_heap,
            index_sizes.implementation,
            0,
            0,
        );
        self[MemberRef].set_columns(
            index_sizes.member_ref_parent,
            index_sizes.string_heap,
            index_sizes.blob_heap,
            0,
            0,
            0,
        );
        self[MethodDef].set_columns(
            4,
            2,
            2,
            index_sizes.string_heap,
            index_sizes.blob_heap,
            index_sizes.param_table,
        );
        self[MethodImpl].set_columns(
            index_sizes.type_def_table,
            index_sizes.method_def_or_ref,
            index_sizes.method_def_or_ref,
            0,
            0,
            0,
        );
        self[MethodSemantics].set_columns(
            2,
            index_sizes.method_def_table,
            index_sizes.has_semantics,
            0,
            0,
            0,
        );
        self[MethodSpec].set_columns(
            index_sizes.method_def_or_ref,
            index_sizes.blob_heap,
            0,
            0,
            0,
            0,
        );

        self[Module].set_columns(
            2,
            index_sizes.string_heap,
            index_sizes.guid_heap,
            index_sizes.guid_heap,
            index_sizes.guid_heap,
            0,
        );
        self[ModuleRef].set_columns(index_sizes.string_heap, 0, 0, 0, 0, 0);
        self[NestedClass].set_columns(
            index_sizes.type_def_table,
            index_sizes.type_def_table,
            0,
            0,
            0,
            0,
        );
        self[Param].set_columns(2, 2, index_sizes.string_heap, 0, 0, 0);
        self[Property].set_columns(2, index_sizes.string_heap, index_sizes.blob_heap, 0, 0, 0);
        self[PropertyMap].set_columns(
            index_sizes.type_def_table,
            index_sizes.property_table,
            0,
            0,
            0,
            0,
        );
        self[StandAloneSig].set_columns(index_sizes.blob_heap, 0, 0, 0, 0, 0);
        self[TypeDef].set_columns(
            4,
            index_sizes.string_heap,
            index_sizes.string_heap,
            index_sizes.type_def_or_ref,
            index_sizes.field_table,
            index_sizes.method_def_table,
        );
        self[TypeRef].set_columns(
            index_sizes.resolution_scope,
            index_sizes.string_heap,
            index_sizes.string_heap,
            0,
            0,
            0,
        );
        self[TypeSpec].set_columns(index_sizes.blob_heap, 0, 0, 0, 0, 0);
        self[CustomDebugInformation].set_columns(
            index_sizes.has_custom_debug_information,
            index_sizes.guid_heap,
            index_sizes.blob_heap,
            0,
            0,
            0,
        );
        self[Document].set_columns(
            index_sizes.blob_heap,
            index_sizes.guid_heap,
            index_sizes.blob_heap,
            index_sizes.guid_heap,
            0,
            0,
        );
        self[ImportScope].set_columns(
            index_sizes.import_scope_table,
            index_sizes.blob_heap,
            0,
            0,
            0,
            0,
        );
        self[LocalConstant].set_columns(index_sizes.string_heap, index_sizes.blob_heap, 0, 0, 0, 0);
        self[LocalScope].set_columns(
            index_sizes.method_def_table,
            index_sizes.import_scope_table,
            index_sizes.local_variable_table,
            index_sizes.local_constant_table,
            4,
            4,
        );
        self[LocalVariable].set_columns(2, 2, index_sizes.string_heap, 0, 0, 0);
        self[MethodDebugInformation].set_columns(
            index_sizes.document_table,
            index_sizes.blob_heap,
            0,
            0,
            0,
            0,
        );
        self[StateMachineMethod].set_columns(
            index_sizes.method_def_table,
            index_sizes.method_def_table,
            0,
            0,
            0,
            0,
        );
    }

    /// Sets the contents of all tables in this stream.
    fn set_contents(&mut self, mut table_contents: &'data [u8]) {
        for table in self.tables.iter_mut() {
            table.set_contents(&mut table_contents);
        }
    }

    /// Returns the size in bytes of an index into this Portable PDB file's `#String` heap.
    ///
    /// See <https://github.com/stakx/ecma-335/blob/master/docs/ii.24.2.6-metadata-stream.md> for an explanation.
    fn string_index_size(&self) -> usize {
        if self.header.heap_sizes & 0x1 == 0 {
            2
        } else {
            4
        }
    }

    /// Returns the size in bytes of an index into this Portable PDB file's `#Guid` heap.
    ///
    /// See <https://github.com/stakx/ecma-335/blob/master/docs/ii.24.2.6-metadata-stream.md> for an explanation.
    fn guid_index_size(&self) -> usize {
        if self.header.heap_sizes & 0x2 == 0 {
            2
        } else {
            4
        }
    }

    /// Returns the size in bytes of an index into this Portable PDB file's `#Blob` heap.
    ///
    /// See <https://github.com/stakx/ecma-335/blob/master/docs/ii.24.2.6-metadata-stream.md> for an explanation.
    fn blob_index_size(&self) -> usize {
        if self.header.heap_sizes & 0x4 == 0 {
            2
        } else {
            4
        }
    }

    fn table_size(&self, table: TableType, referenced_table_sizes: &[u32; 64]) -> usize {
        std::cmp::max(
            self[table].rows,
            referenced_table_sizes[table as usize] as usize,
        )
    }

    /// Returns the size in bytes of an index into this stream's `table` table, based on the table's
    /// number of rows.
    fn table_index_size(&self, table: TableType, referenced_table_sizes: &[u32; 64]) -> usize {
        if self.table_size(table, referenced_table_sizes) >= u16::MAX as usize {
            4
        } else {
            2
        }
    }

    /// Returns the size in bytes of an index into any of the tables in `tables`.
    ///
    /// This depends on the number of tables (because some part of the index needs to be used
    /// as a tag) and the  maximum number of rows among them.
    fn composite_index_size(
        &self,
        tables: &[TableType],
        referenced_table_sizes: &[u32; 64],
    ) -> usize {
        /// Checks if `row_count` is less than 2^(16 - bits).
        fn is_small(row_count: usize, bits: u8) -> bool {
            (row_count as u64) < (1u64 << (16 - bits))
        }

        /// Calculates the number of bits necessary to distinguish between `num_tables` different tables.
        ///
        /// This number is equal to ceil(log₂(num_tables)).
        fn tag_bits(num_tables: usize) -> u8 {
            let mut num_tables = num_tables - 1;
            let mut bits: u8 = 1;
            loop {
                num_tables >>= 1;
                if num_tables == 0 {
                    break;
                }
                bits += 1;
            }
            bits
        }

        let bits_needed = tag_bits(tables.len());
        if tables
            .iter()
            .map(|table| self.table_size(*table, referenced_table_sizes))
            .all(|row_count| is_small(row_count, bits_needed))
        {
            2
        } else {
            4
        }
    }

    /// Returns a record of  `IndexSizes` for this stream.
    fn index_sizes(&self, referenced_table_sizes: &[u32; 64]) -> IndexSizes {
        use TableType::*;
        IndexSizes {
            string_heap: self.string_index_size(),
            guid_heap: self.guid_index_size(),
            blob_heap: self.blob_index_size(),
            assembly_ref_table: self.table_index_size(AssemblyRef, referenced_table_sizes),
            event_table: self.table_index_size(Event, referenced_table_sizes),
            field_table: self.table_index_size(Field, referenced_table_sizes),
            generic_param_table: self.table_index_size(GenericParam, referenced_table_sizes),
            method_def_table: self.table_index_size(MethodDef, referenced_table_sizes),
            module_ref_table: self.table_index_size(ModuleRef, referenced_table_sizes),
            param_table: self.table_index_size(Param, referenced_table_sizes),
            property_table: self.table_index_size(Property, referenced_table_sizes),
            type_def_table: self.table_index_size(TypeDef, referenced_table_sizes),
            document_table: self.table_index_size(Document, referenced_table_sizes),
            import_scope_table: self.table_index_size(ImportScope, referenced_table_sizes),
            local_constant_table: self.table_index_size(LocalConstant, referenced_table_sizes),
            local_variable_table: self.table_index_size(LocalVariable, referenced_table_sizes),
            type_def_or_ref: self
                .composite_index_size(&[TypeDef, TypeRef, TypeSpec], referenced_table_sizes),
            has_constant: self
                .composite_index_size(&[Field, Param, Property], referenced_table_sizes),
            has_custom_attribute: self.composite_index_size(
                &[
                    MethodDef,
                    Field,
                    TypeRef,
                    TypeDef,
                    Param,
                    InterfaceImpl,
                    MemberRef,
                    Module,
                    // the spec lists "Permission" here, but there's no such table?!
                    Property,
                    Event,
                    StandAloneSig,
                    ModuleRef,
                    TypeSpec,
                    Assembly,
                    AssemblyRef,
                    File,
                    ExportedType,
                    ManifestResource,
                    GenericParam,
                    GenericParamConstraint,
                    MethodSpec,
                ],
                referenced_table_sizes,
            ),
            has_field_marshal: self.composite_index_size(&[Field, Param], referenced_table_sizes),
            has_decl_security: self
                .composite_index_size(&[TypeDef, MethodDef, Assembly], referenced_table_sizes),
            member_ref_parent: self.composite_index_size(
                &[TypeDef, TypeRef, ModuleRef, MethodDef, TypeSpec],
                referenced_table_sizes,
            ),
            has_semantics: self.composite_index_size(&[Event, Property], referenced_table_sizes),
            method_def_or_ref: self
                .composite_index_size(&[MethodDef, MemberRef], referenced_table_sizes),
            member_forwarded: self
                .composite_index_size(&[Field, MethodDef], referenced_table_sizes),
            implementation: self
                .composite_index_size(&[File, AssemblyRef, ExportedType], referenced_table_sizes),
            custom_attribute_type: self.composite_index_size(
                &[DummyEmpty, DummyEmpty, MethodDef, MemberRef, DummyEmpty],
                referenced_table_sizes,
            ),
            resolution_scope: self.composite_index_size(
                &[Module, ModuleRef, AssemblyRef, TypeRef],
                referenced_table_sizes,
            ),
            type_or_method_def: self
                .composite_index_size(&[TypeDef, MethodDef], referenced_table_sizes),
            has_custom_debug_information: self.composite_index_size(
                &[
                    MethodDef,
                    Field,
                    TypeRef,
                    TypeDef,
                    Param,
                    InterfaceImpl,
                    MemberRef,
                    Module,
                    DeclSecurity,
                    Property,
                    Event,
                    StandAloneSig,
                    ModuleRef,
                    TypeSpec,
                    Assembly,
                    AssemblyRef,
                    File,
                    ExportedType,
                    ManifestResource,
                    GenericParam,
                    GenericParamConstraint,
                    MethodSpec,
                    Document,
                    LocalScope,
                    LocalVariable,
                    LocalConstant,
                    ImportScope,
                ],
                referenced_table_sizes,
            ),
        }
    }
}

impl<'data> Index<TableType> for MetadataStream<'data> {
    type Output = Table<'data>;

    fn index(&self, index: TableType) -> &Self::Output {
        &self.tables[index as usize]
    }
}

impl IndexMut<TableType> for MetadataStream<'_> {
    fn index_mut(&mut self, index: TableType) -> &mut Self::Output {
        &mut self.tables[index as usize]
    }
}

/// An iterator over CustomDebugInformation of a specific Kind.
/// See [CustomDebugInformation](https://github.com/dotnet/runtime/blob/main/docs/design/specs/PortablePdb-Metadata.md#customdebuginformation-table-0x37).
#[derive(Debug, Clone)]
pub(crate) struct CustomDebugInformationIterator<'data> {
    table: Table<'data>,
    /// Which kind of CustomDebugInformation we want to filter.
    /// We only store the offset in the GUID table to avoid lookups every time.
    kind: Option<u32>,
    /// Current row in the whole table (not just the filtered kind).
    /// Note that the row is 1-based, to align with the rest of the crate APIs.
    row: usize,
}

impl<'data> CustomDebugInformationIterator<'data> {
    pub(crate) fn new(ppdb: &PortablePdb<'data>, filter_kind: Uuid) -> Result<Self, FormatError> {
        let md_stream = ppdb
            .metadata_stream
            .as_ref()
            .ok_or(FormatErrorKind::NoMetadataStream)?;

        let kind = ppdb
            .guid_stream
            .as_ref()
            .ok_or(FormatErrorKind::NoGuidStream)?
            .get_offset(filter_kind);

        Ok(CustomDebugInformationIterator {
            table: md_stream[TableType::CustomDebugInformation],
            kind,
            row: 1,
        })
    }
}

macro_rules! ok_or_return {
    ( $a:expr ) => {
        match $a {
            Ok(value) => value,
            Err(err) => return Some(Err(err)),
        }
    };
}

impl Iterator for CustomDebugInformationIterator<'_> {
    type Item = Result<CustomDebugInformation, FormatError>;

    fn next(&mut self) -> Option<Self::Item> {
        let expected_kind_offset = self.kind?;
        // Find the first row in the table matching the desired Kind.
        while self.row <= self.table.rows {
            let row = ok_or_return!(self.table.get_row(self.row));
            self.row += 1;

            let kind_offset = ok_or_return!(row.get_col_u32(2));

            if kind_offset == expected_kind_offset {
                // Column 1 contains a Parent coded with HasCustomDebugInformation on the lower 5 bits
                let parent = ok_or_return!(row.get_col_u32(1));
                let value = parent >> 5;
                let tag = ok_or_return!(CustomDebugInformationTag::from(parent & 0b11111));

                let blob = ok_or_return!(row.get_col_u32(3));
                return Some(Ok(CustomDebugInformation { tag, value, blob }));
            }
        }
        None
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CustomDebugInformation {
    pub(crate) tag: CustomDebugInformationTag,
    pub(crate) value: u32,
    pub(crate) blob: u32,
}

/// See [CustomDebugInformation](https://github.com/dotnet/runtime/blob/main/docs/design/specs/PortablePdb-Metadata.md#customdebuginformation-table-0x37).
#[derive(Debug, Clone, Copy)]
pub(crate) enum CustomDebugInformationTag {
    MethodDef = 0,
    Field = 1,
    TypeRef = 2,
    TypeDef = 3,
    Param = 4,
    InterfaceImpl = 5,
    MemberRef = 6,
    Module = 7,
    DeclSecurity = 8,
    Property = 9,
    Event = 10,
    StandAloneSig = 11,
    ModuleRef = 12,
    TypeSpec = 13,
    Assembly = 14,
    AssemblyRef = 15,
    File = 16,
    ExportedType = 17,
    ManifestResource = 18,
    GenericParam = 19,
    GenericParamConstraint = 20,
    MethodSpec = 21,
    Document = 22,
    LocalScope = 23,
    LocalVariable = 24,
    LocalConstant = 25,
    ImportScope = 26,
}

impl CustomDebugInformationTag {
    fn from(value: u32) -> Result<Self, FormatError> {
        Ok(match value {
            x if x == Self::MethodDef as u32 => Self::MethodDef,
            x if x == Self::Field as u32 => Self::Field,
            x if x == Self::TypeRef as u32 => Self::TypeRef,
            x if x == Self::TypeDef as u32 => Self::TypeDef,
            x if x == Self::Param as u32 => Self::Param,
            x if x == Self::InterfaceImpl as u32 => Self::InterfaceImpl,
            x if x == Self::MemberRef as u32 => Self::MemberRef,
            x if x == Self::Module as u32 => Self::Module,
            x if x == Self::DeclSecurity as u32 => Self::DeclSecurity,
            x if x == Self::Property as u32 => Self::Property,
            x if x == Self::Event as u32 => Self::Event,
            x if x == Self::StandAloneSig as u32 => Self::StandAloneSig,
            x if x == Self::ModuleRef as u32 => Self::ModuleRef,
            x if x == Self::TypeSpec as u32 => Self::TypeSpec,
            x if x == Self::Assembly as u32 => Self::Assembly,
            x if x == Self::AssemblyRef as u32 => Self::AssemblyRef,
            x if x == Self::File as u32 => Self::File,
            x if x == Self::ExportedType as u32 => Self::ExportedType,
            x if x == Self::ManifestResource as u32 => Self::ManifestResource,
            x if x == Self::GenericParam as u32 => Self::GenericParam,
            x if x == Self::GenericParamConstraint as u32 => Self::GenericParamConstraint,
            x if x == Self::MethodSpec as u32 => Self::MethodSpec,
            x if x == Self::Document as u32 => Self::Document,
            x if x == Self::LocalScope as u32 => Self::LocalScope,
            x if x == Self::LocalVariable as u32 => Self::LocalVariable,
            x if x == Self::LocalConstant as u32 => Self::LocalConstant,
            x if x == Self::ImportScope as u32 => Self::ImportScope,
            _ => return Err(FormatErrorKind::InvalidCustomDebugInformationTag(value).into()),
        })
    }
}
