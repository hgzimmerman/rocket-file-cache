use rocket::response::{Response, Responder};
use rocket::http::{Status, ContentType};
use rocket::request::Request;

use std::result;
use std::sync::Arc;
use std::path::{PathBuf,Path};
use std::io;

use in_memory_file::InMemoryFile;


/// A wrapper around an in-memory file.
/// This struct is created when when a request to the cache is made.
/// The CachedFile knows its path, so it can set the content type when it is serialized to a response.
#[derive(Debug, Clone, PartialEq)]
pub struct CachedFile {
    pub(crate) path: PathBuf,
    pub(crate) file: Arc<InMemoryFile>,
}

impl CachedFile {
    /// Reads the file at the path into a CachedFile.
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<CachedFile> {
        let sized_file: InMemoryFile = InMemoryFile::open(path.as_ref())?;
        Ok(CachedFile {
            path: path.as_ref().to_path_buf(),
            file: Arc::new(sized_file)
        })
    }


    /// The unsafe code required to set the body of a response is encapsulated in this method.
    /// It converts the SizedFile into a raw pointer so its data can be used to set the streamed body
    /// without explicit ownership.
    /// This prevents copying the file, leading to a significant speedup.
    #[inline]
    pub(crate) fn set_response_body(self, response: &mut Response) {
        let file: *const InMemoryFile = Arc::into_raw(self.file);
        unsafe {
            response.set_streamed_body((*file).bytes.as_slice());
            let _ = Arc::from_raw(file); // To prevent a memory leak, an Arc needs to be reconstructed from the raw pointer.
        }
    }
}


/// Streams the cached file to the client. Sets or overrides the Content-Type in
/// the response according to the file's extension if the extension is recognized.
///
/// If you would like to stream a file with a different Content-Type than that implied by its
/// extension, convert the `CachedFile` to a `File`, and respond with that instead.
///
/// Based on NamedFile from rocket::response::NamedFile
impl Responder<'static> for CachedFile {
    fn respond_to(self, _: &Request) -> result::Result<Response<'static>, Status> {
        let mut response = Response::new();
        if let Some(ext) = self.path.extension() {
            if let Some(ct) = ContentType::from_extension(&ext.to_string_lossy()) {
                response.set_header(ct);
            }
        }

        self.set_response_body(&mut response);

        Ok(response)
    }
}

