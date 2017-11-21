
use std::path::Path;
use std::io::BufReader;
use std::fs::File;
use std::io;
use std::io::Read;
use std::fmt;


/// The structure that represents a file in memory.
/// Keeps an up to date record of its stats so the cache can use this information to remove the file
/// from the cache.
#[derive(Clone, PartialEq)]
pub struct InMemoryFile {
    pub(crate) bytes: Vec<u8>,
    pub stats: FileStats,
}

impl fmt::Debug for InMemoryFile {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // The byte array shouldn't be visible in the log.
        write!(
            f,
            "SizedFile {{ bytes: ..., size: {}, priority: {} }}",
            self.stats.size,
            self.stats.priority
        )
    }
}


impl InMemoryFile {
    /// Reads the file at the path into an InMemoryFile.
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<InMemoryFile> {
        let file = File::open(path.as_ref())?;
        let mut reader = BufReader::new(file);
        let mut bytes: Vec<u8> = vec![];
        let size: usize = reader.read_to_end(&mut bytes)?;

        let stats = FileStats {
            size,
            access_count: 0,
            priority: 0,
        };

        Ok(InMemoryFile { bytes, stats })
    }
}


/// Holds information related to the InMemoryFile.
/// This information will be used to determine if the file should be replaced in the cache.
#[derive(Debug, PartialEq, Clone)]
pub struct FileStats {
    pub size: usize,
    pub access_count: usize,
    pub priority: usize,
}