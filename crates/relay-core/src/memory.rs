pub const PAGE_SIZE: u64 = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegionKind {
    Usable,
    Reserved,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryRegion {
    pub start: u64,
    pub end: u64,
    pub kind: RegionKind,
}

impl MemoryRegion {
    pub const fn new(start: u64, end: u64, kind: RegionKind) -> Self {
        Self { start, end, kind }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReservedRange {
    pub start: u64,
    pub end: u64,
}

impl ReservedRange {
    pub const fn new(start: u64, end: u64) -> Self {
        Self { start, end }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryError {
    InvalidRange,
    OverlappingRegion,
}

pub struct FrameAllocator<'a> {
    regions: &'a [MemoryRegion],
    reserved: &'a [ReservedRange],
    region_index: usize,
    next: u64,
}

impl<'a> FrameAllocator<'a> {
    pub fn new(
        regions: &'a [MemoryRegion],
        reserved: &'a [ReservedRange],
    ) -> Result<Self, MemoryError> {
        let mut previous_end = 0;
        for region in regions {
            validate_range(region.start, region.end)?;
            if region.start < previous_end {
                return Err(MemoryError::OverlappingRegion);
            }
            previous_end = region.end;
        }
        for range in reserved {
            validate_range(range.start, range.end)?;
        }
        Ok(Self {
            regions,
            reserved,
            region_index: 0,
            next: regions.first().map_or(0, |region| region.start),
        })
    }

    pub fn allocate_frame(&mut self) -> Option<u64> {
        while let Some(region) = self.regions.get(self.region_index) {
            if region.kind != RegionKind::Usable {
                self.region_index += 1;
                self.next = self
                    .regions
                    .get(self.region_index)
                    .map_or(0, |next| next.start);
                continue;
            }
            if self.next >= region.end {
                self.region_index += 1;
                self.next = self
                    .regions
                    .get(self.region_index)
                    .map_or(0, |next| next.start);
                continue;
            }
            let frame = self.next;
            self.next += PAGE_SIZE;
            if self
                .reserved
                .iter()
                .any(|range| frame >= range.start && frame < range.end)
            {
                continue;
            }
            return Some(frame);
        }
        None
    }
}

fn validate_range(start: u64, end: u64) -> Result<(), MemoryError> {
    if start >= end || !start.is_multiple_of(PAGE_SIZE) || !end.is_multiple_of(PAGE_SIZE) {
        return Err(MemoryError::InvalidRange);
    }
    Ok(())
}
