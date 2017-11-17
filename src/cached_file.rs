use rocket::response::{Response, Responder};
use rocket::http::{Status, ContentType};
use rocket::request::Request;

use std::result;
use std::sync::Arc;
use std::path::{PathBuf,Path};
use std::io;
use std::sync::{Mutex, MutexGuard};

use in_memory_file::InMemoryFile;


/// A wrapper around an in-memory file.
/// This struct is created when when a request to the cache is made.
/// The CachedFile knows its path, so it can set the content type when it is serialized to a response.
#[derive(Debug)]
pub struct CachedFile<'a> {
    pub(crate) path: PathBuf,
    pub(crate) file: Arc<MutexGuard<'a, InMemoryFile>>,
}


#[derive(Debug,)]
pub struct AltCachedFile<'a> {
    pub(crate) path: PathBuf,
    pub(crate) file: Arc<MutexGuard<'a, InMemoryFile>>, // I would need to clone a locked file from a normal CachedFile in order to use this.
    // That would be bad for performance.
}

impl <'a>CachedFile<'a> {
    /// Reads the file at the path into a CachedFile.
//    pub fn open<P: AsRef<Path>>(path: P, m: &mut Mutex<InMemoryFile>) -> io::Result<CachedFile<'static>> {
//        let sized_file: InMemoryFile = InMemoryFile::open(path.as_ref())?;
//
//        m = Mutex::new(sized_file);
//
//        unsafe {
//            let locked_mutex = MutexGuard::new(m);
//        }
//
//        Ok(CachedFile {
//            path: path.as_ref().to_path_buf(),
//            file: Arc::new(m)
//        })
//    }

    pub(crate) fn new<P: AsRef<Path>>(path: P, m: MutexGuard<'a, InMemoryFile>) -> CachedFile<'a> {
        CachedFile {
            path: path.as_ref().to_path_buf(),
            file: Arc::new(m)
        }
    }


    /// The unsafe code required to set the body of a response is encapsulated in this method.
    /// It converts the SizedFile into a raw pointer so its data can be used to set the streamed body
    /// without explicit ownership.
    /// This prevents copying the file, leading to a significant speedup.
//    #[inline]
    pub(crate) fn set_response_body<'b: 'a>(&'a self, response: &'b mut Response) {
//        response.set_streamed_body(self.file.lock().unwrap().bytes.as_slice())

    }
}


/// Streams the cached file to the client. Sets or overrides the Content-Type in
/// the response according to the file's extension if the extension is recognized.
///
/// If you would like to stream a file with a different Content-Type than that implied by its
/// extension, convert the `CachedFile` to a `File`, and respond with that instead.
///
/// Based on NamedFile from rocket::response::NamedFile
impl <'a>Responder<'a> for CachedFile<'a> {

    fn respond_to(self, _: &Request) -> result::Result<Response<'a>, Status> {
        let mut response = Response::new();
        if let Some(ext) = self.path.extension() {
            if let Some(ct) = ContentType::from_extension(&ext.to_string_lossy()) {
                response.set_header(ct);
            }
        }

        unsafe {
            let cloned_wrapper: *const MutexGuard<'a, InMemoryFile> =  Arc::into_raw(self.file);
            response.set_streamed_body((*cloned_wrapper).bytes.as_slice());
            let _ = Arc::from_raw(cloned_wrapper); // To prevent a memory leak, an Arc needs to be reconstructed from the raw pointer.
        }

        Ok(response)
    }
}


