use std::ops::Range;

use ms_pdb::coff::{IMAGE_SIZEOF_SECTION_HEADER, SectionCharacteristics};
use ms_pdb::dbi::optional_dbg::OptionalDebugStream;
use ms_pdb::syms::OffsetSegment;

use super::Pdb;

#[cfg_attr(test, derive(Default))]
pub(super) struct AddressMap {
    section_rvas: Vec<u32>,
    executable: Vec<bool>,
    omap: Option<Vec<(u32, u32)>>,
}

impl AddressMap {
    pub fn new(pdb: &Pdb<'_>) -> anyhow::Result<Self> {
        let sections = pdb.section_headers()?;
        let mut section_rvas = sections
            .iter()
            .map(|section| section.virtual_address)
            .collect();
        let executable = sections
            .iter()
            .map(|section| {
                section.characteristics.intersects(
                    SectionCharacteristics::IMAGE_SCN_CNT_CODE
                        | SectionCharacteristics::IMAGE_SCN_MEM_EXECUTE,
                )
            })
            .collect();
        let omap = if let Some(index) =
            pdb.optional_debug_stream(OptionalDebugStream::ORIGINAL_SECTION_HEADER_DATA)?
        {
            let original = pdb.read_stream(index)?;
            let (headers, remainder) = original.as_chunks::<IMAGE_SIZEOF_SECTION_HEADER>();
            anyhow::ensure!(remainder.is_empty(), "invalid original section headers");
            section_rvas = headers
                .iter()
                .map(|section| u32::from_le_bytes(section[12..16].try_into().unwrap()))
                .collect();
            let index = pdb
                .optional_debug_stream(OptionalDebugStream::OMAP_FROM_SRC_DATA)?
                .ok_or_else(|| anyhow::anyhow!("missing OMAP from source stream"))?;
            let data = pdb.read_stream(index)?;
            let (entries, remainder) = data.as_chunks::<8>();
            anyhow::ensure!(remainder.is_empty(), "invalid OMAP stream length");
            let mut omap = Vec::with_capacity(entries.len());
            for entry in entries {
                let source = u32::from_le_bytes(entry[..4].try_into().unwrap());
                let target = u32::from_le_bytes(entry[4..].try_into().unwrap());
                anyhow::ensure!(
                    omap.last().is_none_or(|&(previous, _)| previous < source),
                    "unsorted OMAP stream"
                );
                omap.push((source, target));
            }
            Some(omap)
        } else {
            None
        };
        Ok(Self {
            section_rvas,
            executable,
            omap,
        })
    }

    pub fn is_executable(&self, offset: OffsetSegment) -> bool {
        offset
            .segment
            .get()
            .checked_sub(1)
            .and_then(|index| self.executable.get(usize::from(index)))
            .copied()
            .unwrap_or(false)
    }

    pub fn internal_rva(&self, offset: OffsetSegment) -> Option<u32> {
        let index = usize::from(offset.segment.get().checked_sub(1)?);
        self.section_rvas
            .get(index)?
            .checked_add(offset.offset.get())
    }

    pub fn rva(&self, offset: OffsetSegment) -> Option<u32> {
        let rva = self.internal_rva(offset)?;
        let Some(omap) = &self.omap else {
            return Some(rva);
        };
        let index = omap
            .partition_point(|&(source, _)| source <= rva)
            .checked_sub(1)?;
        let (source, target) = omap[index];
        if target == 0 {
            None
        } else {
            target.checked_add(rva - source)
        }
    }

    pub fn ranges(&self, range: Range<u32>) -> impl Iterator<Item = Range<u32>> + '_ {
        let omap = self.omap.as_deref().unwrap_or_default();
        let index = omap
            .partition_point(|&(source, _)| source <= range.start)
            .saturating_sub(1);
        let identity = (self.omap.is_none() && !range.is_empty()).then_some(range.clone());
        identity.into_iter().chain(
            omap[index..]
                .iter()
                .enumerate()
                .take_while(move |&(_, &(source, _))| source < range.end)
                .filter_map(move |(relative, &(source, target))| {
                    if target == 0 {
                        return None;
                    }
                    let start = source.max(range.start);
                    let end = omap
                        .get(index + relative + 1)
                        .map_or(range.end, |&(next, _)| next.min(range.end));
                    if start >= end {
                        return None;
                    }
                    Some(target.checked_add(start - source)?..target.checked_add(end - source)?)
                }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pdb_omap_address_and_range_mapping() {
        let map = AddressMap {
            section_rvas: vec![0x1000],
            executable: vec![true],
            omap: Some(vec![(0x1010, 0x2000), (0x1020, 0), (0x1030, 0x3000)]),
        };

        assert!(map.is_executable(OffsetSegment::new(0x14, 1)));
        assert!(!map.is_executable(OffsetSegment::new(0x14, 0)));
        assert_eq!(map.rva(OffsetSegment::new(0x08, 1)), None);
        assert_eq!(map.rva(OffsetSegment::new(0x14, 1)), Some(0x2004));
        assert_eq!(map.rva(OffsetSegment::new(0x24, 1)), None);
        assert_eq!(map.rva(OffsetSegment::new(0x34, 1)), Some(0x3004));
        assert_eq!(
            map.ranges(0x1008..0x1035).collect::<Vec<_>>(),
            [0x2000..0x2010, 0x3000..0x3005]
        );
    }
}
