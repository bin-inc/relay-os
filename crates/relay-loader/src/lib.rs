#![no_std]

extern crate alloc;

mod config;
pub mod cpu;
mod elf;
mod error;
#[cfg(target_os = "uefi")]
pub mod files;
#[cfg(target_os = "uefi")]
pub mod handoff;
#[cfg(any(target_os = "uefi", test))]
pub mod memory;
#[cfg(any(target_os = "uefi", test))]
pub mod paging;
#[cfg(any(target_os = "uefi", test))]
mod post_ebs;
pub mod transition;

pub use config::{LoaderConfig, parse_config};
pub use cpu::{PagingDepth, page_indices, paging_depth_from_cr4};
pub use elf::{ELF_PF_R, ELF_PF_W, ELF_PF_X, LoadPlan, LoadSegment, SegmentFlags, parse_load_plan};
pub use error::{ConfigError, ElfLoadError};
#[cfg(any(target_os = "uefi", test))]
pub use memory::allocate_transition_page;
pub use transition::TransitionPage;
