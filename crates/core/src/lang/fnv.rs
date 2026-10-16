//! A fast hasher for the small tables looked up on every keystroke (one character, one short
//! word). The standard one is built against hash flooding, which tables of ours that no input
//! reaches have no use for.

use std::hash::{BuildHasherDefault, Hasher};

#[derive(Clone, Copy)]
pub struct Fnv(u64);

impl Default for Fnv {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Hasher for Fnv {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = (self.0 ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }
}

pub type FnvBuild = BuildHasherDefault<Fnv>;
