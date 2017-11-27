use rocket::http::Status;
use rocket::response::{Response, Responder, NamedFile};
use rocket::request::Request;
use cache::Cache;
use std::path::Path;

use named_in_memory_file::NamedInMemoryFile;


/// Wrapper around data that can represent a file - either in memory, or on disk.
///
/// When getting a `CachedFile` from the cache:
/// * An `InMemory` variant indicates that the file is now in the cache after the `get()`.
/// * A `FileSystem` variant indicates that the file is not in the cache, but it can be accessed from the filesystem.
/// * A `NotFound` variant indicates that the file can not be found in the filesystem or the cache.
#[derive(Debug)]
pub enum CachedFile<'a> {
    /// A file that has been loaded into the cache.
    InMemory(NamedInMemoryFile<'a>),
    /// A file that exists in the filesystem.
    FileSystem(NamedFile),
    /// The file does not exist in either the cache or the filesystem.
    NotFound
}

impl<'a> CachedFile<'a> {
    /// A convenience function that wraps the getting of a cached file.
    ///
    /// This is done to keep the code required to use the cache as similar to the typical use of
    /// rocket::response::NamedFile.
    pub fn open<P: AsRef<Path>>(path: P, cache: &'a Cache) -> CachedFile<'a> {
        cache.get(path)
    }
}


impl<'a> From<NamedInMemoryFile<'a>> for CachedFile<'a> {
    fn from(cached_file: NamedInMemoryFile<'a>) -> CachedFile<'a> {
        CachedFile::InMemory(cached_file)
    }
}

impl From<NamedFile> for CachedFile<'static> {
    fn from(named_file: NamedFile) -> Self {
        CachedFile::FileSystem(named_file)
    }
}

impl<'a> Responder<'a> for CachedFile<'a> {
    fn respond_to(self, request: &Request) -> Result<Response<'a>, Status> {

        match self {
            CachedFile::InMemory(cached_file) => cached_file.respond_to(request),
            CachedFile::FileSystem(named_file) => named_file.respond_to(request),
            CachedFile::NotFound => {
                error!("Response was `FileNotFound`.",);
                Err(Status::NotFound)
            }
        }
    }
}


impl<'a, 'b> PartialEq for CachedFile<'a> {
    fn eq(&self, other: &CachedFile) -> bool {
        match *self {
            CachedFile::InMemory(ref lhs_cached_file) => {
                match *other {
                    CachedFile::InMemory(ref rhs_cached_file) => (*rhs_cached_file.file).get() == (*lhs_cached_file.file).get(),
                    CachedFile::FileSystem(_) => false,
                    CachedFile::NotFound => false
                }
            }
            CachedFile::FileSystem(ref lhs_named_file) => {
                match *other {
                    CachedFile::InMemory(_) => false,
                    CachedFile::FileSystem(ref rhs_named_file) => {
                        // This just compares the file paths
                        *lhs_named_file.path() == *rhs_named_file.path()
                    }
                    CachedFile::NotFound => false
                }
            }
            CachedFile::NotFound => {
                match *other {
                    CachedFile::InMemory(_) => false,
                    CachedFile::FileSystem(_) => false,
                    CachedFile::NotFound => true
                }
            }
        }

    }
}
