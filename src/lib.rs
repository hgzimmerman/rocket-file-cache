#![feature(test)]

extern crate rocket;
#[macro_use]
extern crate log;

extern crate concurrent_hashmap;

mod cache;
mod in_memory_file;
pub mod named_in_memory_file;
mod cache_builder;
mod priority_function;
mod cached_file;

pub use cache::Cache;
pub use cache_builder::{CacheBuilder, CacheBuildError};
pub use cached_file::CachedFile;
pub use priority_function::*;
