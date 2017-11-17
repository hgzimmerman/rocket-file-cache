use rocket::http::Status;
use rocket::response::{Response, Responder, NamedFile};
use rocket::request::Request;

use super::CachedFile;


/// Wrapper around types that represent files and implement Responder<'static>.
#[derive(Debug)]
pub enum ResponderFile<'a> {
    Cached(CachedFile<'a>),
    FileSystem(NamedFile)
}


impl <'a>From<CachedFile<'a>> for ResponderFile<'a> {
    fn from(cached_file: CachedFile) -> ResponderFile {
        ResponderFile::Cached(cached_file)
    }
}

impl From<NamedFile> for ResponderFile<'static>{
    fn from(named_file: NamedFile) -> Self {
        ResponderFile::FileSystem(named_file)
    }
}

impl  <'a>Responder<'a> for ResponderFile<'a> {
    fn respond_to(self, request: &Request) -> Result<Response<'a>, Status> {

        match self {
            ResponderFile::Cached(cached_file) => cached_file.respond_to(request),
            ResponderFile::FileSystem(named_file) => named_file.respond_to(request)
        }
    }
}


//impl <'a> PartialEq for ResponderFile<'a> {
//    fn eq(&'a self, other: &'a ResponderFile) -> bool {
//        match *self {
//            ResponderFile::Cached(ref lhs_cached_file) => {
//                match *other {
//                    ResponderFile::Cached(ref rhs_cached_file) => {
//                        *rhs_cached_file == *lhs_cached_file
//                    }
//                    ResponderFile::FileSystem(_) => {
//                        false
//                    }
//                }
//            }
//            ResponderFile::FileSystem(ref lhs_named_file) => {
//                match *other {
//                    ResponderFile::Cached(_) => {
//                        false
//                    }
//                    ResponderFile::FileSystem(ref rhs_named_file) => {
//                        // Since all we have is a file handle this will settle for just comparing the paths for now
//                        *lhs_named_file.path() == *rhs_named_file.path()
//                    }
//                }
//            }
//        }
//
//    }
//}