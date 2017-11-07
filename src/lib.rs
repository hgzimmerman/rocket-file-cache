#![feature(plugin)]
#![plugin(rocket_codegen)]
#![feature(test)]

extern crate rocket;
#[macro_use]
extern crate rocket_codegen;


#[macro_use]
extern crate log;
extern crate rand;

use rocket::request::Request;
use rocket::response::{Response, Responder};
use rocket::http::{Status, ContentType};


use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::io::BufReader;
use std::io::{Result, Read};
use std::io;
use std::result;
use std::usize;
use std::fmt;
use std::sync::Arc;

/// The structure that represents a file in memory.
#[derive(Clone)]
pub struct SizedFile {
    bytes: Vec<u8>,
    size: usize
}

/// The byte array shouldn't be visible in the debug log.
impl fmt::Debug for SizedFile {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "SizedFile {{ bytes: ..., size: {} }}", self.size )
    }
}

/// The structure that is returned when a request to the cache is made.
/// The CachedFile structure knows its path, so it can set the extension type when it is serialized to a request.
#[derive(Debug, Clone)]
pub struct CachedFile {
    path: PathBuf,
    file: Arc<SizedFile>

}


impl SizedFile {
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<SizedFile> {
        let file = File::open(path.as_ref())?;
        let mut reader = BufReader::new(file);
        let mut buffer: Vec<u8> = vec!();
        let size: usize = reader.read_to_end(&mut buffer)?;

        Ok(SizedFile {
            bytes: buffer,
            size
        })
    }
}

/// Streams the named file to the client. Sets or overrides the Content-Type in
/// the response according to the file's extension if the extension is
/// recognized. See
/// [ContentType::from_extension](/rocket/http/struct.ContentType.html#method.from_extension)
/// for more information. If you would like to stream a file with a different
/// Content-Type than that implied by its extension, use a `File` directly.
impl Responder<'static> for CachedFile {
    fn respond_to(self, _: &Request) -> result::Result<Response<'static>, Status> {
        let mut response = Response::new();
        if let Some(ext) = self.path.extension() {
            if let Some(ct) = ContentType::from_extension(&ext.to_string_lossy()) {
                response.set_header(ct);
            }
        }

        // Convert the SizedFile into a raw pointer so its data can be used to set the streamed body
        // without explicit ownership.
        // This prevents copying the file, leading to a significant speedup.
        let file: *const SizedFile = Arc::into_raw(self.file);
        unsafe {
            response.set_streamed_body((*file).bytes.as_slice());
            let _ = Arc::from_raw(file); // Prevent dangling pointer?
        }

        Ok(response)
    }
}



/// The Cache holds a set number of files.
/// The Cache acts as a proxy to the filesystem.
/// When a request for a file is made, the Cache checks to see if it has a copy of the file.
/// If it does have a copy, it returns the copy.
/// If it doesn't have a copy, it reads the file from the FS and tries to cache it.
/// If there is room in the Cache, the cache will store the file, otherwise it will increment a count indicating the number of access attempts for the file.
/// If the number of access attempts for the file are higher than the least in demand file in the Cache, the cache will replace the low demand file with the high demand file.
#[derive(Debug)]
pub struct Cache {
    size_limit: usize, // Currently this is being used as the number of elements in the cache, but should be used as the number of bytes in the hashmap.
    file_map: HashMap<PathBuf, Arc<SizedFile>>, // Holds the files that the cache is caching
    access_count_map: HashMap<PathBuf, usize> // Every file that is accessed will have the number of times it is accessed logged in this map.
}


impl Cache {

    /// Creates a new Cache with the given size limit.
    /// Currently the size_limit is representitive of the number of files stored in the Cache, but
    /// plans exist to make size limit represent the maximum number of bytes the Cache's file_map
    /// can hold.
    pub fn new(size_limit: usize) -> Cache {
        Cache {
            size_limit,
            file_map: HashMap::new(),
            access_count_map: HashMap::new()
        }
    }

    /// Attempt to store a given file in the the cache.
    /// Storing will fail if the current files have more access attempts than the file being added.
    /// If the provided file has more more access attempts than one of the files in the cache,
    /// but the cache is full, a file will have to be removed from the cache to make room
    /// for the new file.
    pub fn store(&mut self, path: PathBuf, file: Arc<SizedFile>) -> result::Result<(), String> {
        debug!("Possibly storing file: {:?} in the Cache.");
        // If there is room in the hashmap, just add the file
        if self.size() < self.size_limit {
            self.file_map.insert(path.clone(), file);
            debug!("Inserting a file: {:?} into a not-full cache.", path);
            return Ok(()) // Inserted successfully.
        }

        match self.lowest_access_count_in_file_map() {
            Some(lowest) => {
                let (lowest_count, lowest_key) = lowest;
                // It should early return if a file can be added without having to remove a file first.
                let possible_store_count: usize = *self.access_count_map.get(&path).unwrap_or(&0usize);
                // Currently this removes the file that has been accessed the least.
                // TODO in the future, this should remove the file that has the lowest "score": Access count x sqrt(size)
                if possible_store_count > lowest_count {
                    self.file_map.remove(&lowest_key);
                    self.file_map.insert(path.clone(), file);
                    debug!("Removing file: {:?} to make room for file: {:?}.", lowest_key, path);
                    return Ok(())
                } else {
                    debug!("File: {:?} has less demand than files already in the cache.", path);
                    return Err(String::from("File demand for file is lower than files already in the cache"));
                }
            }
            None => {
                debug!("Inserting first file: {:?} into cache.", path);
                self.file_map.insert(path, file);
                Ok(())
            }
        }
    }

    /// Increments the access count.
    // TODO Currently the access count will be incremented regardless of whether the file exists in the filesystem, consider breaking the increment step into another function.
    ///
    /// Gets the file from the cache if it exists.
    pub fn get(&mut self, path: &PathBuf) -> Option<CachedFile> {
        let count: &mut usize = self.access_count_map.entry(path.to_path_buf()).or_insert(0usize);
        *count += 1; // Increment the access count
        match self.file_map.get(path) {
            Some(sized_file) => {
                Some(
                    CachedFile {
                        path: path.clone(),
                        file: sized_file.clone()
                    }
                )
            }
            None => None

        }

    }

    /// Either gets the file from the cache, gets it from the filesystem and tries to cache it,
    /// or fails to find the file and returns None.

    pub fn get_or_cache(&mut self, pathbuf: PathBuf) -> Option<CachedFile> {
        trace!("{:#?}", self);
        // First try to get the file in the cache that corresponds to the desired path.
        {
            if let Some(cache_file) = self.get(&pathbuf) {
                debug!("Cache hit for file: {:?}", pathbuf);
                return Some(cache_file)
            }
        }

        debug!("Cache missed for file: {:?}", pathbuf);
        // Instead the file needs to read from the filesystem.
        let sized_file: Result<SizedFile> = SizedFile::open(pathbuf.as_path());
        // Check if the file read was a success.
        if let Ok(file) = sized_file {
            // If the file was read, convert it to a cached file and attempt to store it in the cache
            let arc_file = Arc::new(file);
            let cached_file: CachedFile = CachedFile {
                path: pathbuf.clone(),
                file: arc_file.clone()
            };

            let _ = self.store(pathbuf, arc_file); // possibly stores the cached file in the store.
            Some(cached_file)
        } else {
            // Indicate that the file was not found in either the filesystem or cache.
            None
        }
    }

    /// Gets the file with the lowest access count in the hashmap.
    fn lowest_access_count_in_file_map(&self) -> Option<(usize,PathBuf)> {
        if self.file_map.keys().len() == 0 {
            return None
        }

        let mut lowest_access_count: usize = usize::MAX;
        let mut lowest_access_key: PathBuf = PathBuf::new();

        for file_key in self.file_map.keys() {
            let access_count: &usize = self.access_count_map.get(file_key).unwrap(); // It is guaranteed for the access count entry to exist if the file_map entry exists.
            if access_count < &lowest_access_count {
                lowest_access_count = access_count + 0;
                lowest_access_key = file_key.clone();
            }
        }
        Some((lowest_access_count, lowest_access_key))
    }

    /// Gets the number of files in the file_map.
    fn size(&self) -> usize {
        let mut size: usize = 0;
        for _ in self.file_map.keys() {
            size += 1;
        }
        size
    }

    /// Gets the size of the files that constitute the file_map.
    fn size_bytes(&self) -> usize {
        self.file_map.iter().fold(0usize, |size, x| {
            size +  x.1.size
        })
    }

}




#[cfg(test)]
mod tests {
    extern crate test;
    use super::*;

    use std::sync::Mutex;
    use rocket::Rocket;
    use rocket::local::Client;
    use self::test::Bencher;
    use rocket::response::NamedFile;
    use rocket::State;

    #[get("/<path..>", rank=4)]
    fn cache_files(path: PathBuf, cache: State<Mutex<Cache>>) -> Option<CachedFile> {
        let pathbuf: PathBuf = Path::new("test").join(path.clone()).to_owned();
        cache.lock().unwrap().get_or_cache(pathbuf)
    }
    fn init_cache_rocket() -> Rocket {
        let cache: Mutex<Cache> = Mutex::new(Cache::new(10));
        rocket::ignite()
            .manage(cache)
            .mount("/", routes![cache_files])
    }

    #[get("/<file..>")]
    fn fs_files(file: PathBuf) -> Option<NamedFile> {
        NamedFile::open(Path::new("test").join(file)).ok()
    }
    fn init_fs_rocket() -> Rocket {
        rocket::ignite()
            .mount("/", routes![fs_files])
    }

    // generated by running `base64 /dev/urandom | head -c 1000000 > one_meg.txt`
    const ONE_MEG: &'static str = "one_meg.txt"; // file path used for testing
    const FIVE_MEGS: &'static str = "five_megs.txt"; // file path used for testing
    const TEN_MEGS: &'static str = "ten_megs.txt"; // file path used for testing


    #[bench]
    fn cache_access_1mib(b: &mut Bencher) {
        let client = Client::new(init_cache_rocket()).expect("valid rocket instance");
        let _response = client.get(ONE_MEG).dispatch(); // make sure the file is in the cache
        b.iter(|| {
            let mut response = client.get(ONE_MEG).dispatch();
            let _body: Vec<u8> = response.body().unwrap().into_bytes().unwrap();
        });
    }

    #[bench]
    fn file_access_1mib(b: &mut Bencher) {
        let client = Client::new(init_fs_rocket()).expect("valid rocket instance");
        b.iter(|| {
            let mut response = client.get(ONE_MEG).dispatch();
            let _body: Vec<u8> = response.body().unwrap().into_bytes().unwrap();
        });
    }

    #[bench]
    fn cache_access_5mib(b: &mut Bencher) {
        let client = Client::new(init_cache_rocket()).expect("valid rocket instance");
        let _response = client.get(FIVE_MEGS).dispatch(); // make sure the file is in the cache
        b.iter(|| {
            let mut response = client.get(FIVE_MEGS).dispatch();
            let _body: Vec<u8> = response.body().unwrap().into_bytes().unwrap();
        });
    }

    #[bench]
    fn file_access_5mib(b: &mut Bencher) {
        let client = Client::new(init_fs_rocket()).expect("valid rocket instance");
        b.iter(|| {
            let mut response = client.get(FIVE_MEGS).dispatch();
            let _body: Vec<u8> = response.body().unwrap().into_bytes().unwrap();
        });
    }

    #[bench]
    fn cache_access_10mib(b: &mut Bencher) {
        let client = Client::new(init_cache_rocket()).expect("valid rocket instance");
        let _response = client.get(TEN_MEGS).dispatch(); // make sure the file is in the cache
        b.iter(|| {
            let mut response = client.get(TEN_MEGS).dispatch();
            let _body: Vec<u8> = response.body().unwrap().into_bytes().unwrap();
        });
    }

    #[bench]
    fn file_access_10mib(b: &mut Bencher) {
        let client = Client::new(init_fs_rocket()).expect("valid rocket instance");
        b.iter(|| {
            let mut response = client.get(TEN_MEGS).dispatch();
            let _body: Vec<u8> = response.body().unwrap().into_bytes().unwrap();
        });
    }

    // Comparison test
    #[bench]
    fn clone5mib(b: &mut Bencher) {
        use rand::{StdRng, Rng};
        let mut megs2: Box<[u8; 5000000]> = Box::new([0u8; 5000000]);
        StdRng::new().unwrap().fill_bytes(megs2.as_mut());
        b.iter(|| {
            megs2.clone()
        });
    }
}
