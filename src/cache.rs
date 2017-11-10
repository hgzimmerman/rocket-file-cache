use std::path::PathBuf;
use std::collections::HashMap;
use std::sync::Arc;
use std::usize;

use cached_file::CachedFile;
use sized_file::SizedFile;
use priority_function::{PriorityFunction, DEFAULT_PRIORITY_FUNCTION};



#[derive(Debug, PartialEq)]
pub enum CacheInvalidationError {
    NoMoreFilesToRemove,
    NewPriorityIsNotHighEnough,
    NewFileSmallerThanMin,
    NewFileLargerThanMax,
    NewFileLargerThanCache
}

#[derive(Debug, PartialEq)]
pub enum CacheInvalidationSuccess {
    ReplacedFile,
    InsertedFileIntoAvailableSpace,
}

#[derive(Debug, PartialEq, Clone)]
pub struct AccessCountAndPriority {
    access_count: usize,
    priority_score: usize
}


/// The Cache holds a number of files whose bytes fit into its size_limit.
/// The Cache acts as a proxy to the filesystem.
/// When a request for a file is made, the Cache checks to see if it has a copy of the file.
/// If it does have a file, it returns an Arc reference to the file in a format that can easily be serialized.
/// If it doesn't own a copy of the file, it reads the file from the FS and tries to cache it.
/// If there is room in the Cache, the cache will store the file, otherwise it will increment a
/// count indicating the number of access attempts for the file.
///
/// When the cache is full, each file in the cache will have priority score determined by the priority function.
/// When a a new file is attempted to be stored, it will calculate the priority of the new score and
/// compare that against the score of the file with the lowest priority in the cache.
/// If the new file's priority is higher, then the file in the cache will be removed and replaced with the new file.
/// If removing the first file doesn't free up enough space for the new file, then the file with the
/// next lowest priority will have its priority added to the other removed file's and the aggregate
/// cached file's priority will be tested against the new file's.
///
/// This will repeat until either enough space can be freed for the new file, and the new file is
/// inserted, or until the priority of the cached files is greater than that of the new file,
/// in which case, the new file isn't inserted.
#[derive(Debug)]
pub struct Cache {
    pub(crate) size_limit: usize, // The number of bytes the file_map should ever hold.
    pub(crate) min_file_size: usize, // The minimum size file that can be added to the cache
    pub(crate) max_file_size: usize, // The maximum size file that can be added to the cache
    pub(crate) priority_function: PriorityFunction, // The priority function that is used to determine which files should be in the cache.
    pub(crate) file_map: HashMap<PathBuf, Arc<SizedFile>>, // Holds the files that the cache is caching
    pub(crate) count_and_priority_map: HashMap<PathBuf, AccessCountAndPriority>, // Every file that is accessed will have the number of times it is accessed logged in this map.
}


impl Cache {
    /// Creates a new Cache with the given size limit and the default priority function.
    /// The min and max file sizes are not set.
    ///
    /// # Arguments
    ///
    /// * `size_limit` - The number of bytes that the Cache is allowed to hold at a given time.
    ///
    pub fn new(size_limit: usize) -> Cache {
        Cache {
            size_limit,
            min_file_size: 0,
            max_file_size: usize::MAX,
            priority_function: DEFAULT_PRIORITY_FUNCTION,
            file_map: HashMap::new(),
            count_and_priority_map: HashMap::new(),
        }
    }


    /// Attempt to store a given file in the the cache.
    /// Storing will fail if the current files have more access attempts than the file being added.
    /// If the provided file has more more access attempts than one of the files in the cache,
    /// but the cache is full, a file will have to be removed from the cache to make room
    /// for the new file.
    ///
    /// If the insertion works, the cache will update the priority score for the file being inserted.
    /// The cached priority score requires the file in question to exist in the file map, so it will
    /// have a size to use when calculating.
    ///
    /// # Arguments
    ///
    /// * `path` - The path of the file to be stored. Acts as a key for the file in the cache.
    /// * `file` - A file that will be attempted to be stored in the cache.
    ///
    fn try_store(&mut self, path: PathBuf, file: Arc<SizedFile>) -> Result<CacheInvalidationSuccess, CacheInvalidationError> {
        debug!("Possibly storing file: {:?} in the Cache.", path);

        // Don't store the file if is too big or small.
        if file.size > self.max_file_size {
            return Err(CacheInvalidationError::NewFileLargerThanMax)
        }
        if file.size < self.min_file_size {
            return Err(CacheInvalidationError::NewFileSmallerThanMin)
        }
        if file.size > self.size_limit {
            return Err(CacheInvalidationError::NewFileLargerThanCache)
        }

        let required_space_for_new_file: isize = (self.size_bytes() as isize + file.size as isize) - self.size_limit as isize;

        // If there is negative required space, then we can just add the file to the cache, as it will fit.
        if required_space_for_new_file < 0 {
            debug!("Cache has room for the file.");
            self.file_map.insert(path.clone(), file);
            self.update_priority_score(&path);
            Ok(CacheInvalidationSuccess::InsertedFileIntoAvailableSpace)
        } else {
            // Otherwise, the cache will have to try to make some room for the new file

            // The access_count should have incremented since the last time this was called, so the priority must be recalculated.
            // Also, the size generally
            let new_file_access_count: usize = match self.count_and_priority_map.get(&path) {
                Some(count_and_priority) => count_and_priority.access_count,
                None => 1,
            };
            let new_file_priority: usize = (self.priority_function)(new_file_access_count, file.size);


            match self.make_room_for_new_file(required_space_for_new_file as usize, new_file_priority) {
                Ok(_) => {
                    debug!("Made room in the cache for new file. Adding new file to cache.");
                    self.file_map.insert(path.clone(), file);
                    self.update_priority_score(&path);
                    Ok(CacheInvalidationSuccess::ReplacedFile)
                }
                Err(e) => {
                    debug!("The file does not have enough priority or is too large to be accepted into the cache.");
                    return Err(e);

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
    ///
    /// # Arguments
    ///
    /// * `required_space` - A `usize` representing the number of bytes that must be freed to make room for a new file.
    /// * `new_file_priority` - A `usize` representing the priority of the new file to be added. If the priority of the files possibly being removed
    /// is greater than this value, then the files won't be removed.
    fn make_room_for_new_file(&mut self, required_space: usize, new_file_priority: usize) -> Result<(), CacheInvalidationError> {
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
                        return Err( CacheInvalidationError::NewPriorityIsNotHighEnough)
                    }
                }
                None => return Err( CacheInvalidationError::NoMoreFilesToRemove),
            };
        }

        // If this hasn't returned early, then the files to remove are less important than the new file.
        for file_key in file_paths_to_remove {
            self.file_map.remove(&file_key);
        }
        return Ok(());
    }

    ///Helper function that gets the file from the cache if it exists there.
    fn get_from_cache(&mut self, path: &PathBuf) -> Option<CachedFile> {
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
        let count_and_priority: &mut AccessCountAndPriority = self.count_and_priority_map.entry(path.to_path_buf()).or_insert(
            // By default, the count and priority will be 0.
            // The count will immediately be incremented, and the score can't be calculated without the size of the file in question.
            // Therefore, files not in the cache MUST have their priority score calculated on insertion attempt.
            AccessCountAndPriority {
                access_count: 0,
                priority_score: 0
            },
        );
        count_and_priority.access_count += 1; // Increment the access count
    }


    fn update_priority_score(&mut self, path: &PathBuf) {
        let file_size: usize = match self.get_from_cache(path){
            Some(cached_file) => cached_file.file.size,
            None => {
                0
            }
        };

        let count_and_priority: &mut AccessCountAndPriority = self.count_and_priority_map.entry(path.to_path_buf()).or_insert(
            // By default, the count and priority will be 0.
            // The count will immediately be incremented, and the score can't be calculated without the size of the file in question.
            // Therefore, files not in the cache MUST have their priority score calculated on insertion attempt.
            AccessCountAndPriority {
                access_count: 0,
                priority_score: 0
            },
        );

        count_and_priority.priority_score = (self.priority_function)(count_and_priority.access_count, file_size); // update the priority score.
    }

    /// Either gets the file from the cache if it exists there, gets it from the filesystem and
    /// tries to cache it, or fails to find the file and returns None.
    ///
    /// # Arguments
    ///
    /// * `pathbuf` - A pathbuf that represents the path of the file in the fileserver. The pathbuf
    /// also acts as a key for the file in the cache.
    pub fn get_or_cache(&mut self, pathbuf: PathBuf) -> Option<CachedFile> {
        trace!("{:#?}", self);
        // First, try to get the file in the cache that corresponds to the desired path.
        {
            if let Some(cache_file) = self.get_from_cache(&pathbuf) {
                debug!("Cache hit for file: {:?}", pathbuf);
                self.increment_access_count(&pathbuf); // File is in the cache, increment the count
                self.update_priority_score(&pathbuf);
                return Some(cache_file);
            }
        }

        debug!("Cache missed for file: {:?}", pathbuf);
        // Instead the file needs to read from the filesystem.
        if let Ok(file) = SizedFile::open(pathbuf.as_path()) {
            // Because the file exists, but is not in the cache, increment the access count.
            self.increment_access_count(&pathbuf);
            // If the file was read, convert it to a cached file and attempt to store it in the cache
            let arc_file: Arc<SizedFile> = Arc::new(file);
            let cached_file: CachedFile = CachedFile {
                path: pathbuf.clone(),
                file: arc_file.clone(),
            };

            // possibly stores the cached file in the store.
            let _ = self.try_store(pathbuf.clone(), arc_file);

            Some(cached_file)
        } else {
            // Indicate that the file was not found in either the filesystem or cache.
            // This None is interpreted by Rocket by default to forward the request to its 404 handler.
            None
        }
    }


    // TODO having to sort the priorities kills performance on caches with many cached files.
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
                let size: usize = sized_file.size;

                let count_and_priority: AccessCountAndPriority = self.count_and_priority_map
                    .get(file_key)
                    .unwrap_or(
                        &AccessCountAndPriority{
                            access_count: 1,
                            priority_score: (self.priority_function)(1, size) // The priority function needs to be ran, because the priority score _must_ be set here to prevent immediate, incorrect invalidation.
                        }
                    )
                    .clone();

                (file_key.clone(), count_and_priority.priority_score, size)
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

}



#[cfg(test)]
mod tests {
    extern crate test;
    extern crate tempdir;
    extern crate rand;

    use super::*;

    use self::tempdir::TempDir;
    use self::test::Bencher;
    use self::rand::{StdRng, Rng};
    use std::io::{Write, BufWriter};
    use std::fs::File;
    use rocket::response::NamedFile;
    use std::io::Read;


    const MEG1: usize = 1024 * 1024;
    const MEG2: usize = MEG1 * 2;
    const MEG5: usize = MEG1 * 5;
    const MEG10: usize = MEG1 * 10;

    const DIR_TEST: &'static str = "test1";
    const FILE_MEG1: &'static str = "meg1.txt";
    const FILE_MEG2: &'static str = "meg2.txt";
    const FILE_MEG5: &'static str = "meg5.txt";
    const FILE_MEG10: &'static str = "meg10.txt";


    #[bench]
    fn cache_get_10mb(b: &mut Bencher) {
        let mut cache: Cache = Cache::new(MEG1 *20); //Cache can hold 20Mb
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_10m = create_test_file(&temp_dir, MEG10, FILE_MEG10);
        cache.get_or_cache(path_10m.clone()); // add the 10 mb file to the cache

        b.iter(|| {
            let cached_file = cache.get_or_cache(path_10m.clone()).unwrap();
            // Mimic what is done when the response body is set.
            let file: *const SizedFile = Arc::into_raw(cached_file.file);
            unsafe {
                let _ = (*file).bytes.clone();
                let _ = Arc::from_raw(file); // Prevent dangling pointer?
            }
        });
    }

    #[bench]
    fn cache_miss_10mb(b: &mut Bencher) {
        let mut cache: Cache = Cache::new(0);
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_10m = create_test_file(&temp_dir, MEG10, FILE_MEG10);

        b.iter(|| {
            let cached_file = cache.get_or_cache(path_10m.clone()).unwrap();
            // Mimic what is done when the response body is set.
            let file: *const SizedFile = Arc::into_raw(cached_file.file);
            unsafe {
                let _ = (*file).bytes.clone();
                let _ = Arc::from_raw(file); // Prevent dangling pointer?
            }
        });
    }

    #[bench]
    fn named_file_read_10mb(b: &mut Bencher) {
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_10m = create_test_file(&temp_dir, MEG10, FILE_MEG10);

        b.iter(|| {
            let mut named_file = NamedFile::open(path_10m.clone()).unwrap();
            let mut v :Vec<u8> = vec![];
            named_file.read_to_end(&mut v).unwrap();
        });
    }

    #[bench]
    fn cache_get_1mb(b: &mut Bencher) {
        let mut cache: Cache = Cache::new(MEG1 *20); //Cache can hold 20Mb
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_1m = create_test_file(&temp_dir, MEG1, FILE_MEG1);
        cache.get_or_cache(path_1m.clone()); // add the 10 mb file to the cache

        b.iter(|| {
            let cached_file = cache.get_or_cache(path_1m.clone()).unwrap();
            // Mimic what is done when the response body is set.
            let file: *const SizedFile = Arc::into_raw(cached_file.file);
            unsafe {
                let _ = (*file).bytes.clone();
                let _ = Arc::from_raw(file); // Prevent dangling pointer?
            }
        });
    }

    #[bench]
    fn cache_miss_1mb(b: &mut Bencher) {
        let mut cache: Cache = Cache::new(0);
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_1m = create_test_file(&temp_dir, MEG1, FILE_MEG1);

        b.iter(|| {
            let cached_file = cache.get_or_cache(path_1m.clone()).unwrap();
            // Mimic what is done when the response body is set.
            let file: *const SizedFile = Arc::into_raw(cached_file.file);
            unsafe {
                let _ = (*file).bytes.clone();
                let _ = Arc::from_raw(file); // Prevent dangling pointer?
            }
        });
    }

    #[bench]
    fn named_file_read_1mb(b: &mut Bencher) {
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_1m = create_test_file(&temp_dir, MEG1, FILE_MEG1);

        b.iter(|| {
            let mut named_file = NamedFile::open(path_1m.clone()).unwrap();
            let mut v :Vec<u8> = vec![];
            named_file.read_to_end(&mut v).unwrap();
        });
    }



    #[bench]
    fn cache_get_5mb(b: &mut Bencher) {
        let mut cache: Cache = Cache::new(MEG1 *20); //Cache can hold 20Mb
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_5m = create_test_file(&temp_dir, MEG5, FILE_MEG5);
        cache.get_or_cache(path_5m.clone()); // add the 10 mb file to the cache

        b.iter(|| {
            let cached_file = cache.get_or_cache(path_5m.clone()).unwrap();
            // Mimic what is done when the response body is set.
            let file: *const SizedFile = Arc::into_raw(cached_file.file);
            unsafe {
                let _ = (*file).bytes.clone();
                let _ = Arc::from_raw(file); // Prevent dangling pointer?
            }
        });
    }

    #[bench]
    fn cache_miss_5mb(b: &mut Bencher) {
        let mut cache: Cache = Cache::new(0);
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_5m = create_test_file(&temp_dir, MEG5, FILE_MEG5);

        b.iter(|| {
            let cached_file = cache.get_or_cache(path_5m.clone()).unwrap();
            // Mimic what is done when the response body is set.
            let file: *const SizedFile = Arc::into_raw(cached_file.file);
            unsafe {
                let _ = (*file).bytes.clone();
                let _ = Arc::from_raw(file); // Prevent dangling pointer?
            }
        });
    }

    #[bench]
    fn named_file_read_5mb(b: &mut Bencher) {
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_5m = create_test_file(&temp_dir, MEG5, FILE_MEG5);

        b.iter(|| {
            let mut named_file = NamedFile::open(path_5m.clone()).unwrap();
            let mut v :Vec<u8> = vec![];
            named_file.read_to_end(&mut v).unwrap();
        });
    }

    // Constant time access regardless of size.
    #[bench]
    fn cache_get_1mb_from_1000_entry_cache(b: &mut Bencher) {
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_1m = create_test_file(&temp_dir, MEG1, FILE_MEG1);
        let mut cache: Cache = Cache::new(MEG1 *3); //Cache can hold 3Mb
        cache.get_or_cache(path_1m.clone()); // add the file to the cache

        // Add 1024 1kib files to the cache.
        for i in 0..1024 {
            let path = create_test_file(&temp_dir, 1024, format!("{}_1kib.txt", i).as_str());
            // make sure that the file has a high priority.
            for _ in 0..10000 {
                cache.get_or_cache(path.clone());
            }        }

        assert_eq!(cache.size_bytes(), MEG1 * 2);

        b.iter(|| {
            let cached_file = cache.get_or_cache(path_1m.clone()).unwrap();
            // Mimic what is done when the response body is set.
            let file: *const SizedFile = Arc::into_raw(cached_file.file);
            unsafe {
                let _ = (*file).bytes.clone();
                let _ = Arc::from_raw(file); // Prevent dangling pointer?
            }
        });
    }

    // There is a penalty for missing the cache.
    #[bench]
    fn cache_miss_1mb_from_1000_entry_cache(b: &mut Bencher) {
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_1m = create_test_file(&temp_dir, MEG1, FILE_MEG1);
        let mut cache: Cache = Cache::new(MEG1 ); //Cache can hold 1Mb

        // Add 1024 1kib files to the cache.
        for i in 0..1024 {
            let path = create_test_file(&temp_dir, 1024, format!("{}_1kib.txt", i).as_str());
            // make sure that the file has a high priority.
            for _ in 0..1000 {
                cache.get_or_cache(path.clone());
            }
        }

        b.iter(|| {
            let cached_file = cache.get_or_cache(path_1m.clone()).unwrap();
            // Mimic what is done when the response body is set.
            let file: *const SizedFile = Arc::into_raw(cached_file.file);
            unsafe {
                let _ = (*file).bytes.clone();
                let _ = Arc::from_raw(file); // Prevent dangling pointer?
            }
        });
    }

    // This is pretty much a worst-case scenario, where every file would have to be removed to make room for the new file.
    // There is a penalty for missing the cache.
    #[bench]
    fn cache_miss_5mb_from_1000_entry_cache(b: &mut Bencher) {
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_5m = create_test_file(&temp_dir, MEG5, FILE_MEG1);
        let mut cache: Cache = Cache::new(MEG5 ); //Cache can hold 1Mb

        // Add 1024 5kib files to the cache.
        for i in 0..1024 {
            let path = create_test_file(&temp_dir, 1024 * 5, format!("{}_5kib.txt", i).as_str());
            // make sure that the file has a high priority.
            for _ in 0..1000 {
                cache.get_or_cache(path.clone());
            }
        }

        b.iter(|| {
            let cached_file = cache.get_or_cache(path_5m.clone()).unwrap();
            // Mimic what is done when the response body is set.
            let file: *const SizedFile = Arc::into_raw(cached_file.file);
            unsafe {
                let _ = (*file).bytes.clone();
                let _ = Arc::from_raw(file); // Prevent dangling pointer?
            }
        });
    }


    #[test]
    fn file_exceeds_size_limit() {
        let mut cache: Cache = Cache::new(MEG1 * 8); // Cache can hold only 8Mb
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_10m = create_test_file(&temp_dir, MEG10, FILE_MEG10);
        assert_eq!(
            cache.try_store(
                path_10m.clone(),
                Arc::new(SizedFile::open(path_10m.clone()).unwrap()),
            ),
            Err(CacheInvalidationError::NewFileLargerThanCache)
        )
    }

    #[test]
    fn file_replaces_other_file() {
        let temp_dir = TempDir::new(DIR_TEST).unwrap();

        let path_1m = create_test_file(&temp_dir, MEG1, FILE_MEG1);
        let path_5m = create_test_file(&temp_dir, MEG5, FILE_MEG5);


        let mut cache: Cache = Cache::new(5500000); //Cache can hold only 5.5Mib
        cache.increment_access_count(&path_5m);
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
        cache.update_priority_score(&path_5m);
        assert_eq!(
            cache.try_store(
                path_5m.clone(),
                Arc::new(SizedFile::open(path_5m.clone()).unwrap()),
            ),
            Ok(CacheInvalidationSuccess::InsertedFileIntoAvailableSpace)
        );

        cache.increment_access_count(&path_2m);
        cache.update_priority_score(&path_2m);
        assert_eq!(
            cache.try_store(
                path_2m.clone(),
                Arc::new(SizedFile::open(path_2m.clone()).unwrap()),
            ),
            Ok(CacheInvalidationSuccess::InsertedFileIntoAvailableSpace)
        );

        // The cache will not accept the 1 meg file because sqrt(2)_size * 1_access is greater than sqrt(1)_size * 1_access
        cache.increment_access_count(&path_1m);
        cache.update_priority_score(&path_1m);
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
        cache.update_priority_score(&path_1m);
        assert_eq!(
            cache.try_store(
                path_1m.clone(),
                Arc::new(SizedFile::open(path_1m.clone()).unwrap()),
            ),
            Ok(CacheInvalidationSuccess::ReplacedFile)
        );

        if let None = cache.get_from_cache(&path_1m) {
            assert_eq!(&path_1m, &PathBuf::new()) // this will fail, this comparison is just for debugging a failure.
        }

        // Get directly from the cache, no FS involved.
        if let None = cache.get_from_cache(&path_5m) {
            assert_eq!(&path_5m, &PathBuf::new()) // this will fail, this comparison is just for debugging a failure.
            // If this has failed, the cache removed the wrong file, implying the ordering of
            // priorities is wrong. It should remove the path_2m file instead.
        }

        if let Some(_) = cache.get_from_cache(&path_2m) {
            assert_eq!(&path_2m, &PathBuf::new()) // this will fail, this comparison is just for debugging a failure.
        }
    }
}
