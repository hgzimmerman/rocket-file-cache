use rocket::http::{Status, ContentType};
use rocket::response::{Response, Responder, NamedFile};
use rocket::request::Request;

use super::CachedFile;


/// Wrapper around types that represent files and implement Responder<'static>.
#[derive(Debug)]
pub enum ResponderFile {
    Cached(CachedFile),
    FileSystem(NamedFile)
}


impl From<CachedFile> for ResponderFile {
    fn from(cached_file: CachedFile) -> Self {
        ResponderFile::Cached(cached_file)
    }
}

impl From<NamedFile> for ResponderFile {
    fn from(named_file: NamedFile) -> Self {
        ResponderFile::FileSystem(named_file)
    }
}

impl Responder<'static> for ResponderFile {
    fn respond_to(self, request: &Request) -> Result<Response<'static>, Status> {

        match self {
            ResponderFile::Cached(cached_file) => cached_file.respond_to(request),
            ResponderFile::FileSystem(named_file) => named_file.respond_to(request)
        }
    }
}


impl PartialEq for ResponderFile {
    fn eq(&self, other: &ResponderFile) -> bool {
        match *self {
            ResponderFile::Cached(ref lhs_cached_file) => {
                match *other {
                    ResponderFile::Cached(ref rhs_cached_file) => {
                        *rhs_cached_file == *lhs_cached_file
                    }
                    ResponderFile::FileSystem(_) => {
                        false
                    }
                }
            }
            ResponderFile::FileSystem(ref lhs_named_file) => {
                match *other {
                    ResponderFile::Cached(_) => {
                        false
                    }
                    ResponderFile::FileSystem(ref rhs_named_file) => {
                        // Since all we have is a file handle this will settle for just comparing the paths for now
                        *lhs_named_file.path() == *rhs_named_file.path()
                    }
                }
            }
        }

    }
}