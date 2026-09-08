#![no_std]

extern crate alloc;

mod config;
mod elf;
mod error;

pub use config::{LoaderConfig, parse_config};
pub use elf::{ELF_PF_R, ELF_PF_W, ELF_PF_X, LoadPlan, LoadSegment, SegmentFlags, parse_load_plan};
pub use error::{ConfigError, ElfLoadError};
