#![feature(test)]
extern crate rocket;
#[macro_use]
extern crate log;



use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::io::BufReader;
use std::io::Read;
use std::io;
use std::result;
use std::usize;
use std::fmt;
use std::sync::Arc;

pub mod cache;
mod sized_file;
pub mod cached_file;
pub mod cache_builder;








/// Custom type of function that is used to determine how to add files to the cache.
/// The first term will be assigned the access count of the file in question, while the second term will be assigned the size (in bytes) of the file in question.
/// The result will represent the priority of the file to remain in or be added to the cache.
/// The files with the largest priorities will be kept in the cache.
///
/// A closure that matches this type signature can be specified at cache instantiation to define how it will keep items in the cache.
pub type PriorityFunction = fn(usize, usize) -> usize;



