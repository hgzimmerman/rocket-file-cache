#![feature(test)]

extern crate rocket;
#[macro_use]
extern crate log;
extern crate either;


pub mod cache;
mod in_memory_file;
pub mod cached_file;
pub mod cache_builder;
pub mod priority_function;

pub use cache::Cache;
pub use cache_builder::{CacheBuilder, CacheBuildError};
pub use cached_file::{CachedFile, ResponderFile};
pub use priority_function::*;









