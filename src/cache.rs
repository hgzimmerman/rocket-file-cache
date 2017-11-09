use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::sync::Arc;

use cached_file::CachedFile;
use sized_file::SizedFile;
use super::PriorityFunction;



#[derive(Debug, PartialEq)]
pub enum CacheInvalidationError {
    NoMoreFilesToRemove,
    NewPriorityIsNotHighEnough,
}

#[derive(Debug, PartialEq)]
pub enum CacheInvalidationSuccess {
    ReplacedFile,
    InsertedFileIntoAvailableSpace,
}



/// The Cache holds a number of files whose bytes fit into its size_limit.
/// The Cache acts as a proxy to the filesystem.
/// When a request for a file is made, the Cache checks to see if it has a copy of the file.
/// If it does have a copy, it returns the copy.
/// If it doesn't have a copy, it reads the file from the FS and tries to cache it.
/// If there is room in the Cache, the cache will store the file, otherwise it will increment a count indicating the number of access attempts for the file.
/// If the number of access attempts for the file are higher than the least in demand file in the Cache, the cache will replace the low demand file with the high demand file.
#[derive(Debug)]
pub struct Cache {
    pub(crate) size_limit: usize, // The number of bytes the file_map should ever hold.
    pub(crate) priority_function: PriorityFunction, // The priority function that is used to determine which files should be in the cache.
    pub(crate) file_map: HashMap<PathBuf, Arc<SizedFile>>, // Holds the files that the cache is caching
    pub(crate) access_count_map: HashMap<PathBuf, usize>, // Every file that is accessed will have the number of times it is accessed logged in this map.
}


impl Cache {
    //TODO, consider moving to the builder pattern if min and max file sizes are added as options.
    /// Creates a new Cache with the given size limit and the default priority function.
    pub fn new(size_limit: usize) -> Cache {
        Cache {
            size_limit,
            priority_function: Cache::DEFAULT_PRIORITY_FUNCTION,
            file_map: HashMap::new(),
            access_count_map: HashMap::new(),
        }
    }


    /// Attempt to store a given file in the the cache.
    /// Storing will fail if the current files have more access attempts than the file being added.
    /// If the provided file has more more access attempts than one of the files in the cache,
    /// but the cache is full, a file will have to be removed from the cache to make room
    /// for the new file.
    pub fn try_store(&mut self, path: PathBuf, file: Arc<SizedFile>) -> Result<CacheInvalidationSuccess, CacheInvalidationError> {
        debug!("Possibly storing file: {:?} in the Cache.", path);

        let required_space_for_new_file: isize = (self.size_bytes() as isize + file.size as isize) - self.size_limit as isize;

        // If there is negative required space, then we can just add the file to the cache, as it will fit.
        if required_space_for_new_file < 0 {
            debug!("Cache has room for the file.");
            self.file_map.insert(path, file);
            Ok(CacheInvalidationSuccess::InsertedFileIntoAvailableSpace)
        } else {
            // Otherwise, the cache will have to try to make some room for the new file

            let new_file_access_count: usize = *self.access_count_map.get(&path).unwrap_or(&0usize);
            let new_file_priority: usize = (self.priority_function)(new_file_access_count, file.size);


            match self.make_room_for_new_file(required_space_for_new_file as usize, new_file_priority) {
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
    ///
    /// If this returns an OK, this function has removed the required file space from the file_map.
    /// If this returns an Err, then either not enough space could be freed, or the priority of
    /// files that would need to be freed to make room for the new file is greater than the
    /// new file's priority, and as result no memory was freed.
    fn make_room_for_new_file(&mut self, required_space: usize, new_file_priority: usize) -> Result<(), String> {
        // TODO come up with a better result type.
        let mut possibly_freed_space: usize = 0;
        let mut priority_score_to_free: usize = 0;
        let mut file_paths_to_remove: Vec<PathBuf> = vec![];

        let mut priorities: Vec<(PathBuf, usize, usize)> = self.sorted_priorities();
        while possibly_freed_space < required_space {
            // pop the priority group with the lowest priority off of the vector
            match priorities.pop() {
                Some(lowest) => {
                    let (lowest_key, lowest_file_priority, lowest_file_size) = lowest;

                    possibly_freed_space += lowest_file_size;
                    priority_score_to_free += lowest_file_priority;
                    file_paths_to_remove.push(lowest_key.clone());

                    // Check if total priority to free is greater than the new file's priority,
                    // If it is, then don't free the files, as they in aggregate, are more important
                    // than the new file.
                    if priority_score_to_free > new_file_priority {
                        return Err(String::from(
                            "Priority of new file isn't higher than the aggregate priority of the file(s) it would replace",
                        ));
                    }
                }
                None => return Err(String::from("No more files to remove")),
            };
        }

        // If this hasn't returned early, then the files to remove are less important than the new file.
        for file_key in file_paths_to_remove {
            self.file_map.remove(&file_key);
        }
        return Ok(());
    }

    ///Helper function that gets the file from the cache if it exists there.
    fn get(&mut self, path: &PathBuf) -> Option<CachedFile> {
        match self.file_map.get(path) {
            Some(sized_file) => {
                Some(CachedFile {
                    path: path.clone(),
                    file: sized_file.clone(),
                })
            }
            None => None, // File not found
        }

    }

    /// Helper function for incrementing the access count for a given file name.
    ///
    /// This should only be used in cases where the file is known to exist, to avoid bloating the access count map with useless values.
    fn increment_access_count(&mut self, path: &PathBuf) {
        let count: &mut usize = self.access_count_map.entry(path.to_path_buf()).or_insert(
            0usize,
        );
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
                return Some(cache_file);
            }
        }

        debug!("Cache missed for file: {:?}", pathbuf);
        // Instead the file needs to read from the filesystem.
        if let Ok(file) = SizedFile::open(pathbuf.as_path()) {
            // TODO, consider moving the increment count into try_store()
            self.increment_access_count(&pathbuf); // Because the file exists, but is not in the cache, increment the access count
            // If the file was read, convert it to a cached file and attempt to store it in the cache
            let arc_file: Arc<SizedFile> = Arc::new(file);
            let cached_file: CachedFile = CachedFile {
                path: pathbuf.clone(),
                file: arc_file.clone(),
            };

            let _ = self.try_store(pathbuf, arc_file); // possibly stores the cached file in the store.
            Some(cached_file)
        } else {
            // Indicate that the file was not found in either the filesystem or cache.
            // This None is interpreted by Rocket by default to forward the request to its 404 handler.
            None
        }
    }

    /// Gets a vector of tuples containing the Path, priority score, and size in bytes of all items
    /// in the file_map.
    ///
    /// The vector is sorted from highest to lowest priority.
    /// This allows the assumption that the last element to be popped from the vector will have the
    /// lowest priority, and therefore is the most eligible candidate for elimination from the
    /// cache.
    fn sorted_priorities(&self) -> Vec<(PathBuf, usize, usize)> {

        let mut priorities: Vec<(PathBuf, usize, usize)> = self.file_map
            .iter()
            .map(|file| {
                let (file_key, sized_file) = file;
                let access_count: usize = self.access_count_map
                    .get(file_key)
                    .unwrap_or(&1usize)
                    .clone();
                let size: usize = sized_file.size;
                let priority: usize = (self.priority_function)(access_count, size);

                (file_key.clone(), priority, size)
            })
            .collect();

        // Sort the priorities from highest priority to lowest, so when they are pop()ed later,
        // the last element will have the lowest priority.
        priorities.sort_by(|l, r| r.1.cmp(&l.1));
        priorities
    }


    /// Gets the size of the files that constitute the file_map.
    fn size_bytes(&self) -> usize {
        self.file_map.iter().fold(0usize, |size, x| size + x.1.size)
    }



    /// The default priority function used for determining if a file should be in the cache
    /// This function takes the square root of the size of the file times the number of times it has been accessed.
    fn balanced_priority(access_count: usize, size: usize) -> usize {
        ((size as f64).sqrt() as usize) * access_count
    }
    pub const DEFAULT_PRIORITY_FUNCTION: PriorityFunction = Cache::balanced_priority;

    /// This priority function will value files in the cache based solely on the number of times the file is accessed.
    fn access_priority(access_count: usize, _: usize) -> usize {
        access_count
    }
    pub const ACCESS_PRIORITY_FUNCTION: PriorityFunction = Cache::access_priority;
}




#[cfg(test)]
mod tests {
    extern crate test;
    extern crate tempdir;
    extern crate rand;

    use self::tempdir::TempDir;

    use super::*;

    //    use std::sync::Mutex;
    //    use rocket::Rocket;
    //    use rocket::local::Client;
    use self::test::Bencher;
    //    use rocket::response::NamedFile;
    //    use rocket::State;
    use self::rand::{StdRng, Rng};
    use std::io::{Write, BufWriter};
    use std::fs::File;




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
        let mut megs2: Box<[u8; 5000000]> = Box::new([0u8; 5000000]);
        StdRng::new().unwrap().fill_bytes(megs2.as_mut());

        b.iter(|| megs2.clone());
    }

    #[test]
    fn file_exceeds_size_limit() {
        let mut cache: Cache = Cache::new(MEG1 * 8); //Cache can hold only 8Mb
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_10m = create_test_file(&temp_dir, MEG10, FILE_MEG10);
        assert_eq!(
            cache.try_store(
                path_10m.clone(),
                Arc::new(SizedFile::open(path_10m.clone()).unwrap()),
            ),
            Err(CacheInvalidationError::NewPriorityIsNotHighEnough)
        )
    }

    #[test]
    fn file_replaces_other_file() {
        let temp_dir = TempDir::new(DIR_TEST).unwrap();

        let path_1m = create_test_file(&temp_dir, MEG1, FILE_MEG1);
        let path_5m = create_test_file(&temp_dir, MEG5, FILE_MEG5);


        let mut cache: Cache = Cache::new(5500000); //Cache can hold only 5.5Mib
        assert_eq!(
            cache.try_store(
                path_5m.clone(),
                Arc::new(SizedFile::open(path_5m.clone()).unwrap()),
            ),
            Ok(CacheInvalidationSuccess::InsertedFileIntoAvailableSpace)
        );
        cache.increment_access_count(&path_1m); // increment the access count, causing it to have a higher priority the next time it tries to be stored.
        assert_eq!(
            cache.try_store(
                path_1m.clone(),
                Arc::new(SizedFile::open(path_1m.clone()).unwrap()),
            ),
            Err(CacheInvalidationError::NewPriorityIsNotHighEnough)
        );
        cache.increment_access_count(&path_1m);
        assert_eq!(
            cache.try_store(
                path_1m.clone(),
                Arc::new(SizedFile::open(path_1m.clone()).unwrap()),
            ),
            Err(CacheInvalidationError::NewPriorityIsNotHighEnough)
        );
        cache.increment_access_count(&path_1m);
        assert_eq!(
            cache.try_store(
                path_1m.clone(),
                Arc::new(SizedFile::open(path_1m.clone()).unwrap()),
            ),
            Ok(CacheInvalidationSuccess::ReplacedFile)
        );
    }

    const MEG1: usize = 1024 * 1024;
    const MEG2: usize = MEG1 * 2;
    const MEG3: usize = MEG1 * 3;
    const MEG5: usize = MEG1 * 5;
    const MEG10: usize = MEG1 * 10;

    const DIR_TEST: &'static str = "test1";
    const FILE_MEG1: &'static str = "meg1.txt";
    const FILE_MEG2: &'static str = "meg2.txt";
    const FILE_MEG3: &'static str = "meg3.txt";
    const FILE_MEG5: &'static str = "meg5.txt";
    const FILE_MEG10: &'static str = "meg10.txt";

    // Helper function that creates test files in a directory that is cleaned up after the test runs.
    fn create_test_file(temp_dir: &TempDir, size: usize, name: &str) -> PathBuf {
        let path = temp_dir.path().join(name);
        let tmp_file = File::create(path.clone()).unwrap();
        let mut rand_data: Vec<u8> = vec![0u8; size];
        StdRng::new().unwrap().fill_bytes(rand_data.as_mut());
        let mut buffer = BufWriter::new(tmp_file);
        buffer.write(&rand_data).unwrap();
        path
    }

    #[test]
    fn new_file_replaces_lowest_priority_file() {
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_1m = create_test_file(&temp_dir, MEG1, FILE_MEG1);
        let path_2m = create_test_file(&temp_dir, MEG2, FILE_MEG2);
        //        let path_3m = create_test_file(&temp_dir, MEG3, FILE_MEG3);
        let path_5m = create_test_file(&temp_dir, MEG5, FILE_MEG5);
        //        let path_10m = create_test_file(&temp_dir, MEG10, FILE_MEG10);

        let mut cache: Cache = Cache::new(MEG1 * 7 + 2000);

        cache.increment_access_count(&path_5m);
        assert_eq!(
            cache.try_store(
                path_5m.clone(),
                Arc::new(SizedFile::open(path_5m.clone()).unwrap()),
            ),
            Ok(CacheInvalidationSuccess::InsertedFileIntoAvailableSpace)
        );

        cache.increment_access_count(&path_2m);
        assert_eq!(
            cache.try_store(
                path_2m.clone(),
                Arc::new(SizedFile::open(path_2m.clone()).unwrap()),
            ),
            Ok(CacheInvalidationSuccess::InsertedFileIntoAvailableSpace)
        );

        // The cache will not accept the 1 meg file because sqrt(2)_size * 1_access is greater than sqrt(1)_size * 1_access
        cache.increment_access_count(&path_1m);
        assert_eq!(
            cache.try_store(
                path_1m.clone(),
                Arc::new(SizedFile::open(path_1m.clone()).unwrap()),
            ),
            Err(CacheInvalidationError::NewPriorityIsNotHighEnough)
        );

        // The cache will now accept the 1 meg file because (sqrt(2)_size * 1_access) for the old
        // file is less than (sqrt(1)_size * 2_access) for the new file.
        cache.increment_access_count(&path_1m);
        assert_eq!(
            cache.try_store(
                path_1m.clone(),
                Arc::new(SizedFile::open(path_1m.clone()).unwrap()),
            ),
            Ok(CacheInvalidationSuccess::ReplacedFile)
        );

        if let None = cache.get(&path_1m) {
            assert_eq!(&path_1m, &PathBuf::new()) // this will fail, this comparison is just for debugging a failure.
        }

        // Get directly from the cache, no FS involved.
        if let None = cache.get(&path_5m) {
            assert_eq!(&path_5m, &PathBuf::new()) // this will fail, this comparison is just for debugging a failure.
            // If this has failed, the cache removed the wrong file, implying the ordering of
            // priorities is wrong. It should remove the path_2m file instead.
        }

        if let Some(_) = cache.get(&path_2m) {
            assert_eq!(&path_2m, &PathBuf::new()) // this will fail, this comparison is just for debugging a failure.
        }
    }
}
