//#![feature(plugin)]
//#![plugin(rocket_codegen)]
#![feature(test)]

extern crate rocket;


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
use std::io::Read;
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

impl fmt::Debug for SizedFile {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // The byte array shouldn't be visible in the log.
        write!(f, "SizedFile {{ bytes: ..., size: {} }}", self.size )
    }
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


/// The structure that is returned when a request to the cache is made.
/// The CachedFile knows its path, so it can set the content type when it is serialized to a request.
#[derive(Debug, Clone)]
pub struct CachedFile {
    path: PathBuf,
    file: Arc<SizedFile>
}



/// Streams the named file to the client. Sets or overrides the Content-Type in
/// the response according to the file's extension if the extension is
/// recognized. See
/// [ContentType::from_extension](/rocket/http/struct.ContentType.html#method.from_extension)
/// for more information. If you would like to stream a file with a different
/// Content-Type than that implied by its extension, use a `File` directly.
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

#[derive(Debug, PartialEq)]
pub enum CacheInvalidationError {
    NoMoreFilesToRemove,
    NewPriorityIsNotHighEnough
}

#[derive(Debug, PartialEq)]
pub enum CacheInvalidationSuccess {
    ReplacedFile,
    SpaceAvailableInsertedFile
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
    priority_function: PriorityFunction,
    file_map: HashMap<PathBuf, Arc<SizedFile>>, // Holds the files that the cache is caching
    access_count_map: HashMap<PathBuf, usize> // Every file that is accessed will have the number of times it is accessed logged in this map.
}


impl Cache {

    /// Creates a new Cache with the given size limit.
    /// Currently the size_limit is representative of the number of files stored in the Cache, but
    /// plans exist to make size limit represent the maximum number of bytes the Cache's file_map
    /// can hold.
    pub fn new(size_limit: usize) -> Cache {
        Cache {
            size_limit,
            priority_function: Cache::DEFAULT_PRIORITY_FUNCTION,
            file_map: HashMap::new(),
            access_count_map: HashMap::new()
        }
    }

    /// Attempt to store a given file in the the cache.
    /// Storing will fail if the current files have more access attempts than the file being added.
    /// If the provided file has more more access attempts than one of the files in the cache,
    /// but the cache is full, a file will have to be removed from the cache to make room
    /// for the new file.
    pub fn store(&mut self, path: PathBuf, file: Arc<SizedFile>) -> result::Result<CacheInvalidationSuccess, CacheInvalidationError> {
        debug!("Possibly storing file: {:?} in the Cache.", path);

        let required_space_for_new_file: isize =  (self.size_bytes() as isize + file.size as isize) - self.size_limit as isize;

        // If there is negative required space, then we can just add the file to the cache, as it will fit.
        if required_space_for_new_file < 0 {
            debug!("Cache has room for the file.");
            self.file_map.insert(path, file);
            Ok(CacheInvalidationSuccess::SpaceAvailableInsertedFile)
        } else { // Otherwise, the cache will have to try to make some room for the new file

            let new_file_access_count: usize = *self.access_count_map.get(&path).unwrap_or(&0usize);
            let new_file_priority: usize = (self.priority_function)(new_file_access_count, file.size);


            match self.make_room_for_new_file(required_space_for_new_file as usize , new_file_priority) {
                Ok(_) => {
                    debug!("Made room in the cache for file and is now adding it");
                    self.file_map.insert(path, file);
                    Ok(CacheInvalidationSuccess::ReplacedFile)
                }
                Err(_) => {
                    debug!("The file does not have enough priority or is too large to be accepted into the cache.");
                    return Err(CacheInvalidationError::NewPriorityIsNotHighEnough);

                }
            }
        }
    }

    /// Remove the n lowest priority files to make room for a file with a size: required_space.
    fn make_room_for_new_file(&mut self, required_space: usize, new_file_priority: usize) -> result::Result<(), String> { // TODO come up with a better result type.
        let mut possibly_freed_space: usize = 0;
        let mut priority_score_to_free: usize = 0;
        let mut file_paths_to_remove: Vec<PathBuf> = vec!();
        let mut lowest_file_index: usize = 0; // we need an index into the n lowest priority files.

        // Loop until the more bytes can freed than the number of bytes that are required.
        while possibly_freed_space < required_space {
            match self.lowest_priority_in_file_map(lowest_file_index) {
                Some(lowest) => {
                    let (lowest_key, lowest_file_priority, lowest_file_size) = lowest;

                    possibly_freed_space += lowest_file_size;
                    priority_score_to_free += lowest_file_priority;
                    file_paths_to_remove.push(lowest_key.clone());

                    // Check if total priority to free is greater than the new file's priority,
                    // If it is, then don't free the files, as they in aggregate, are more important
                    // than the new file.
                    if priority_score_to_free > new_file_priority {
                        return Err(String::from("Priority isn't high enough"))
                    }

                }
                None => {
                    return Err(String::from("No more files to remove.")); // There arent any more files to store OR the file to store is too big TODO catch this edge case
                }
            }

            // Increment the lowest file index, so the next time lowest_priority_in_file_map() runs,
            // It will access the file with the next lowest priority.
            lowest_file_index += 1;
        }

        // If this hasn't returned early, then the files to remove are less important than the new file.
        for file in file_paths_to_remove {
            self.file_map.remove(&file);
        }
        return Ok(());
    }

    /// Increments the access count.
    /// Gets the file from the cache if it exists.
    pub fn get(&mut self, path: &PathBuf) -> Option<CachedFile> {
//        self.increment_access_count(path);
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

    /// Helper function for incrementing the access count for a given file name.
    ///
    /// This should only be used in cases where the file is known to exist, to avoid bloating the access count map with useless values.
    fn increment_access_count(&mut self, path: &PathBuf) {
        let count: &mut usize = self.access_count_map.entry(path.to_path_buf()).or_insert(0usize);
        *count += 1; // Increment the access count
    }

    /// Either gets the file from the cache, gets it from the filesystem and tries to cache it,
    /// or fails to find the file and returns None.
    pub fn get_or_cache(&mut self, pathbuf: PathBuf) -> Option<CachedFile> {
        trace!("{:#?}", self);
        // First, try to get the file in the cache that corresponds to the desired path.
        {
            if let Some(cache_file) = self.get(&pathbuf) {
                debug!("Cache hit for file: {:?}", pathbuf);
                self.increment_access_count(&pathbuf); // File is in the cache, increment the count
                return Some(cache_file)
            }
        }

        debug!("Cache missed for file: {:?}", pathbuf);
        // Instead the file needs to read from the filesystem.
        if let Ok(file) = SizedFile::open(pathbuf.as_path()) {
            self.increment_access_count(&pathbuf); // Because the file exists, but is not in the cache, increment the access count
            // If the file was read, convert it to a cached file and attempt to store it in the cache
            let arc_file: Arc<SizedFile> = Arc::new(file);
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

    fn lowest_priority_in_file_map(&self, index: usize) -> Option<(PathBuf,usize,usize)> {
        if self.file_map.keys().len() == 0 {
            return None
        }

        let mut priorities: Vec<(PathBuf,usize,usize)> = self.file_map.iter().map(|file| {
            let (file_key, sized_file) = file;
            let access_count: usize = self.access_count_map.get(file_key).unwrap_or(&1usize).clone();
            let size: usize = sized_file.size;
            let priority: usize = (self.priority_function)(access_count, size);

            (file_key.clone(), priority, size)
        }).collect();

        priorities.sort_by(|l,r| l.1.cmp(&r.1)); // sort by priority
//        println!("Priorities: {:?}", priorities);
        return Some(priorities.remove(index)); // TODO, verify that the list is sorted correctly so the index is extracting the LOWEST priority
    }


    /// Gets the size of the files that constitute the file_map.
    fn size_bytes(&self) -> usize {
        self.file_map.iter().fold(0usize, |size, x| {
            size +  x.1.size
        })
    }



    /// The default priority function used for determining if a file should be in the cache
    /// This function takes the square root of the size of the file times the number of times it has been accessed.
    fn balanced_priority(access_count: usize, size: usize ) -> usize {
        ((size as f64).sqrt() as usize) * access_count
    }
    pub const DEFAULT_PRIORITY_FUNCTION: PriorityFunction = Cache::balanced_priority;

    /// This priority function will value files in the cache based solely on the number of times the file is accessed.
    fn access_priority( access_count: usize, _ : usize) -> usize {
        access_count
    }
    pub const ACCESS_PRIORITY_FUNCTION: PriorityFunction = Cache::access_priority;

}


/// Custom type of function that is used to determine how to add files to the cache.
/// The first term will be assigned the access count of the file in question, while the second term will be assigned the size (in bytes) of the file in question.
/// The result will represent the priority of the file to remain in or be added to the cache.
/// The files with the largest priorities will be kept in the cache.
///
/// A closure that matches this type signature can be specified at cache instantiation to define how it will keep items in the cache.
pub type PriorityFunction = fn(usize, usize) -> usize;




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

//    #[get("/<path..>", rank=4)]
//    fn cache_files(path: PathBuf, cache: State<Mutex<Cache>>) -> Option<CachedFile> {
//        let pathbuf: PathBuf = Path::new("test").join(path.clone()).to_owned();
//        cache.lock().unwrap().get_or_cache(pathbuf)
//    }
//    fn init_cache_rocket() -> Rocket {
//        let cache: Mutex<Cache> = Mutex::new(Cache::new(11000000)); // Cache can hold 11Mib
//        rocket::ignite()
//            .manage(cache)
//            .mount("/", routes![cache_files])
//    }
//
//    #[get("/<file..>")]
//    fn fs_files(file: PathBuf) -> Option<NamedFile> {
//        NamedFile::open(Path::new("test").join(file)).ok()
//    }
//    fn init_fs_rocket() -> Rocket {
//        rocket::ignite()
//            .mount("/", routes![fs_files])
//    }


    // generated by running `base64 /dev/urandom | head -c 1000000 > one_meg.txt`
    const ONE_MEG: &'static str = "one_meg.txt"; // file path used for testing
    const FIVE_MEGS: &'static str = "five_megs.txt"; // file path used for testing
    const TEN_MEGS: &'static str = "ten_megs.txt"; // file path used for testing

//
//    #[bench]
//    fn cache_access_1mib(b: &mut Bencher) {
//        let client = Client::new(init_cache_rocket()).expect("valid rocket instance");
//        let _response = client.get(ONE_MEG).dispatch(); // make sure the file is in the cache
//        b.iter(|| {
//            let mut response = client.get(ONE_MEG).dispatch();
//            let _body: Vec<u8> = response.body().unwrap().into_bytes().unwrap();
//        });
//    }
//
//    #[bench]
//    fn file_access_1mib(b: &mut Bencher) {
//        let client = Client::new(init_fs_rocket()).expect("valid rocket instance");
//        b.iter(|| {
//            let mut response = client.get(ONE_MEG).dispatch();
//            let _body: Vec<u8> = response.body().unwrap().into_bytes().unwrap();
//        });
//    }
//
//    #[bench]
//    fn cache_access_5mib(b: &mut Bencher) {
//        let client = Client::new(init_cache_rocket()).expect("valid rocket instance");
//        let _response = client.get(FIVE_MEGS).dispatch(); // make sure the file is in the cache
//        b.iter(|| {
//            let mut response = client.get(FIVE_MEGS).dispatch();
//            let _body: Vec<u8> = response.body().unwrap().into_bytes().unwrap();
//        });
//    }
//
//    #[bench]
//    fn file_access_5mib(b: &mut Bencher) {
//        let client = Client::new(init_fs_rocket()).expect("valid rocket instance");
//        b.iter(|| {
//            let mut response = client.get(FIVE_MEGS).dispatch();
//            let _body: Vec<u8> = response.body().unwrap().into_bytes().unwrap();
//        });
//    }
//
//    #[bench]
//    fn cache_access_10mib(b: &mut Bencher) {
//        let client = Client::new(init_cache_rocket()).expect("valid rocket instance");
//        let _response = client.get(TEN_MEGS).dispatch(); // make sure the file is in the cache
//        b.iter(|| {
//            let mut response = client.get(TEN_MEGS).dispatch();
//            let _body: Vec<u8> = response.body().unwrap().into_bytes().unwrap();
//        });
//    }
//
//    #[bench]
//    fn file_access_10mib(b: &mut Bencher) {
//        let client = Client::new(init_fs_rocket()).expect("valid rocket instance");
//        b.iter(|| {
//            let mut response = client.get(TEN_MEGS).dispatch();
//            let _body: Vec<u8> = response.body().unwrap().into_bytes().unwrap();
//        });
//    }

    // Comparison test
//    #[bench]
    fn clone5mib(b: &mut Bencher) {
        use rand::{StdRng, Rng};
        let mut megs2: Box<[u8; 5000000]> = Box::new([0u8; 5000000]);
        StdRng::new().unwrap().fill_bytes(megs2.as_mut());

        b.iter(|| {
            megs2.clone()
        });
    }

    #[test]
    fn file_exceeds_size_limit() {
        let mut cache: Cache = Cache::new(8000000); //Cache can hold only 8Mib
        let path: PathBuf = PathBuf::from("test/".to_owned()+TEN_MEGS);
        assert_eq!(cache.store(path.clone(), Arc::new(SizedFile::open(path.clone()).unwrap())), Err(CacheInvalidationError::NewPriorityIsNotHighEnough))
    }

    #[test]
    fn file_replaces_other_file() {
        let mut cache: Cache = Cache::new(5500000); //Cache can hold only 5.5Mib
        let path_5: PathBuf = PathBuf::from("test/".to_owned()+FIVE_MEGS);
        let path_1: PathBuf = PathBuf::from("test/".to_owned()+ONE_MEG);
        assert_eq!(
            cache.store(path_5.clone(), Arc::new(SizedFile::open(path_5.clone()).unwrap())),
            Ok(CacheInvalidationSuccess::SpaceAvailableInsertedFile)
        );
        cache.increment_access_count(&path_1); // increment the access count, causing it to have a higher priority the next time it tries to be stored.
        assert_eq!(
            cache.store(path_1.clone(), Arc::new(SizedFile::open(path_1.clone()).unwrap())),
            Err(CacheInvalidationError::NewPriorityIsNotHighEnough)
        );
        cache.increment_access_count(&path_1);
        assert_eq!(
            cache.store(path_1.clone(), Arc::new(SizedFile::open(path_1.clone()).unwrap())),
            Err(CacheInvalidationError::NewPriorityIsNotHighEnough)
        );
        cache.increment_access_count(&path_1);
        assert_eq!(
            cache.store(path_1.clone(), Arc::new(SizedFile::open(path_1.clone()).unwrap())),
            Ok(CacheInvalidationSuccess::ReplacedFile)
        );
    }
}
