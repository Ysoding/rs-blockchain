use anyhow::Result;

use crate::Block;

pub struct Blockchain {
    pub blocks: Vec<Block>,
}

impl Blockchain {
    pub fn new() -> Self {
        Self {
            blocks: vec![Block::new_genesis_block()],
        }
    }

    pub fn add_block(&mut self, data: String) -> Result<()> {
        let prev_block = &self.blocks[self.blocks.len() - 1];

        let new_block = Block::new(data, prev_block.hash.clone())?;
        self.blocks.push(new_block);
        Ok(())
    }
}
