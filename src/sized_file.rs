
use std::path::Path;
use std::io::BufReader;
use std::fs::File;
use std::io;
use std::io::Read;
use std::fmt;


// TODO consider adding the priority score to this struct.
// currently, when determining if a file can be added, the priority score must be calculated for
// every file in the cache.
// If every file had it pre-computed, either here, as part of the access count map, or as a new
// hashmap, then this expensive operation wouldn't have to be performed.
// That would take a O(n) operation on inserts, and make it a O(1) operation on gets and inserts,
// which would lead to more consistent performance.
/// The structure that represents a file in memory.
/// Keeps a copy of the size of the file so the size can be used in calculating if it should be
/// removed from the cache.
#[derive(Clone)]
pub struct SizedFile {
    pub bytes: Vec<u8>,
    pub size: usize,
}

impl fmt::Debug for SizedFile {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // The byte array shouldn't be visible in the log.
        write!(f, "SizedFile {{ bytes: ..., size: {} }}", self.size)
    }
}


impl SizedFile {
    /// Reads the file at the path into a SizedFile.
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<SizedFile> {
        let file = File::open(path.as_ref())?;
        let mut reader = BufReader::new(file);
        let mut buffer: Vec<u8> = vec![];
        let size: usize = reader.read_to_end(&mut buffer)?;

        Ok(SizedFile {
            bytes: buffer,
            size,
        })
    }
}