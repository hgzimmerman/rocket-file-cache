use rocket::http::Status;
use rocket::response::{Response, Responder, NamedFile};
use rocket::request::Request;
use cache::Cache;
use std::path::Path;

use super::NamedInMemoryFile;


/// Wrapper around types that represent files and implement Responder<'static>.
#[derive(Debug)]
pub enum CachedFile<'a> {
    Cached(NamedInMemoryFile<'a>),
    FileSystem(NamedFile)
}

impl <'a> CachedFile <'a> {
    pub fn open<P: AsRef<Path>>(path: P, cache: &'a mut Cache ) -> Option<CachedFile<'a>> {
        cache.get(path)
    }
}


impl <'a>From<NamedInMemoryFile<'a>> for CachedFile<'a> {
    fn from(cached_file: NamedInMemoryFile<'a>) -> CachedFile<'a> {
        CachedFile::Cached(cached_file)
    }
}

impl From<NamedFile> for CachedFile<'static>{
    fn from(named_file: NamedFile) -> Self {
        CachedFile::FileSystem(named_file)
    }
}

impl  <'a>Responder<'a> for CachedFile<'a> {
    fn respond_to(self, request: &Request) -> Result<Response<'a>, Status> {

        match self {
            CachedFile::Cached(cached_file) => cached_file.respond_to(request),
            CachedFile::FileSystem(named_file) => named_file.respond_to(request)
        }
    }
}


impl <'a, 'b> PartialEq for CachedFile<'a> {
    fn eq(&self, other: &CachedFile) -> bool {
        match *self {
            CachedFile::Cached(ref lhs_cached_file) => {
                match *other {
                    CachedFile::Cached(ref rhs_cached_file) => {
                        use std::ops::Deref;
                        (*rhs_cached_file.file).deref() == (*lhs_cached_file.file).deref()
                    }
                    CachedFile::FileSystem(_) => {
                        false
                    }
                }
            }
            CachedFile::FileSystem(ref lhs_named_file) => {
                match *other {
                    CachedFile::Cached(_) => {
                        false
                    }
                    CachedFile::FileSystem(ref rhs_named_file) => {
                        // Since all we have is a file handle this will settle for just comparing the paths for now
                        *lhs_named_file.path() == *rhs_named_file.path()
                    }
                }
            }
        }

    }
}