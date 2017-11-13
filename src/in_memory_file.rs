
use std::path::Path;
use std::io::BufReader;
use std::fs::File;
use std::io;
use std::io::Read;
use std::fmt;



/// The structure that represents a file in memory.
/// Keeps a copy of the size of the file so the size can be used in calculating if it should be
/// removed from the cache.
#[derive(Clone, PartialEq)]
pub(crate) struct InMemoryFile {
    pub bytes: Vec<u8>,
    pub size: usize,
}

impl fmt::Debug for InMemoryFile {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // The byte array shouldn't be visible in the log.
        write!(f, "SizedFile {{ bytes: ..., size: {} }}", self.size)
    }
}


impl InMemoryFile {
    /// Reads the file at the path into an InMemoryFile.
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<InMemoryFile> {
        let file = File::open(path.as_ref())?;
        let mut reader = BufReader::new(file);
        let mut buffer: Vec<u8> = vec![];
        let size: usize = reader.read_to_end(&mut buffer)?;

        Ok(InMemoryFile {
            bytes: buffer,
            size,
        })
    }
}