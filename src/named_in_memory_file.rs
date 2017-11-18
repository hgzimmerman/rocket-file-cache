use rocket::response::{Response, Responder};
use rocket::http::{Status, ContentType};
use rocket::request::Request;

use std::result;
use std::sync::Arc;
use std::path::{PathBuf,Path};
use std::sync::{MutexGuard};

use in_memory_file::InMemoryFile;

use concurrent_hashmap::Accessor;

use std::fmt::{Formatter, Debug};
use std::fmt;


/// A wrapper around an in-memory file.
/// This struct is created when when a request to the cache is made.
/// The CachedFile knows its path, so it can set the content type when it is serialized to a response.
//#[derive(Debug)]
pub struct NamedInMemoryFile<'a> {
    pub(crate) path: PathBuf,
    pub(crate) file: Arc<Accessor<'a, PathBuf, Arc<InMemoryFile>>>,
}


impl<'a> Debug for NamedInMemoryFile<'a> {
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        write!(fmt, "path: {:?}, file: {:?}", self.path, self.file.get())
    }
}


impl <'a> NamedInMemoryFile<'a> {
    /// Reads the file at the path into a CachedFile.
    pub(crate) fn new<P: AsRef<Path>>(path: P, m: Accessor<'a, PathBuf, Arc<InMemoryFile>>) -> NamedInMemoryFile<'a> {
        NamedInMemoryFile {
            path: path.as_ref().to_path_buf(),
            file: Arc::new(m)
        }
    }
    // TODO, create a macro that reads an in-memory-file, creates a mutex, and locks the file in the mutex, and creates the cached file.
}


/// Streams the cached file to the client. Sets or overrides the Content-Type in
/// the response according to the file's extension if the extension is recognized.
///
/// If you would like to stream a file with a different Content-Type than that implied by its
/// extension, convert the `CachedFile` to a `File`, and respond with that instead.
///
/// Based on NamedFile from rocket::response::NamedFile
impl <'a>Responder<'a> for NamedInMemoryFile<'a> {

    fn respond_to(self, _: &Request) -> result::Result<Response<'a>, Status> {
        let mut response = Response::new();
        if let Some(ext) = self.path.extension() {
            if let Some(ct) = ContentType::from_extension(&ext.to_string_lossy()) {
                response.set_header(ct);
            }
        }

        unsafe {
            let cloned_wrapper: *const Accessor<'a, PathBuf, Arc<InMemoryFile>> =  Arc::into_raw(self.file);
            response.set_streamed_body((*cloned_wrapper).get().bytes.as_slice());
            let _ = Arc::from_raw(cloned_wrapper); // To prevent a memory leak, an Arc needs to be reconstructed from the raw pointer.
        }

        Ok(response)
    }
}


