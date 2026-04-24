use crate::{MdxError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordBlockInfo {
    pub compressed_size: u64,
    pub decompressed_size: u64,
    pub compressed_accumulator_offset: u64,
    pub decompressed_accumulator_offset: u64,
}

impl RecordBlockInfo {
    pub fn decompressed_end(&self) -> Result<u64> {
        self.decompressed_accumulator_offset
            .checked_add(self.decompressed_size)
            .ok_or_else(|| MdxError::InvalidFormat("record block range overflow".into()))
    }

    fn contains(&self, offset: u64) -> Result<bool> {
        Ok(self.decompressed_accumulator_offset <= offset && offset < self.decompressed_end()?)
    }
}

#[derive(Debug, Clone)]
pub struct RecordRangeTree {
    blocks: Vec<RecordBlockInfo>,
}

impl RecordRangeTree {
    pub fn new(mut blocks: Vec<RecordBlockInfo>) -> Result<Self> {
        blocks.sort_by_key(|block| block.decompressed_accumulator_offset);
        for pair in blocks.windows(2) {
            if pair[0].decompressed_end()? > pair[1].decompressed_accumulator_offset {
                return Err(MdxError::InvalidFormat(
                    "overlapping record block ranges".into(),
                ));
            }
        }
        Ok(Self { blocks })
    }

    pub fn query(&self, offset: u64) -> Option<&RecordBlockInfo> {
        let idx = self
            .blocks
            .partition_point(|block| block.decompressed_accumulator_offset <= offset);
        idx.checked_sub(1).and_then(|candidate| {
            let block = &self.blocks[candidate];
            match block.contains(offset) {
                Ok(true) => Some(block),
                Ok(false) | Err(_) => None,
            }
        })
    }

    pub fn len(&self) -> usize {
        self.blocks.len()
    }
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }
    pub fn blocks(&self) -> &[RecordBlockInfo] {
        &self.blocks
    }
}
