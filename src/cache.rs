use std::path::{PathBuf, Path};
use std::usize;
use rocket::response::NamedFile;
use std::fs::Metadata;
use std::fs;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use named_in_memory_file::NamedInMemoryFile;
use cached_file::CachedFile;
use in_memory_file::InMemoryFile;
use priority_function::{default_priority_function};
use concurrent_hashmap::ConcHashMap;
use std::collections::hash_map::RandomState;
use std::fmt::Debug;
use std::fmt;
use std::fmt::Formatter;

#[derive(Debug, PartialEq)]
enum CacheError {
    NoMoreFilesToRemove,
    NewPriorityIsNotHighEnough,
    InvalidMetadata,
    InvalidPath,
}


#[derive(Debug, PartialEq, Clone)]
pub struct FileStats {
    pub size: usize,
    pub access_count: usize,
    pub priority: usize,
}



/// The cache holds a number of files whose bytes fit into its size_limit.
/// The cache acts as a proxy to the filesystem, returning cached files if they are in the cache,
/// or reading a file directly from the filesystem if the file is not in the cache.
///
/// When the cache is full, each file in the cache will have priority score determined by a provided
/// priority function.
/// When a call to `get()` is made, an access counter for the file in question is incremented,
/// usually increasing the priority score of the file.
/// When a a new file is attempted to be stored, it will calculate the priority of the new score and
/// compare that against the score of the file with the lowest priority in the cache.
/// If the new file's priority is higher, then the file in the cache will be removed and replaced
/// with the new file.
/// If removing the first file doesn't free up enough space for the new file, then the file with the
/// next lowest priority will have its priority added to the other removed file's and the aggregate
/// cached file's priority will be tested against the new file's.
///
/// This will repeat until either enough space can be freed for the new file, and the new file is
/// inserted, or until the priority of the cached files is greater than that of the new file,
/// in which case, the new file isn't inserted.
pub struct Cache {
    /// The number of bytes the file_map should be able hold at once.
    pub size_limit: usize,
    pub min_file_size: usize, // The minimum size file that can be added to the cache
    pub max_file_size: usize, // The maximum size file that can be added to the cache
    pub priority_function: fn(usize, usize) -> usize, // The priority function that is used to determine which files should be in the cache.
    pub(crate) file_map: ConcHashMap<PathBuf, InMemoryFile, RandomState>, // Holds the files that the cache is caching
    pub(crate) access_count_map: ConcHashMap<PathBuf, AtomicUsize, RandomState>, // Every file that is accessed will have the number of times it is accessed logged in this map.
}

unsafe impl Send for Cache {}
unsafe impl Sync for Cache {}

impl Debug for Cache {
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        fmt.debug_map()
            .entries(self.file_map.iter().map(
                |(ref k, ref v)| (k.clone(), v.clone()),
            ))
            .finish()
    }
}

impl Cache {
    /// Creates a new Cache with the given size limit, no limits on individual file size, and the default priority function.
    /// These settings can be set by using the CacheBuilder instead.
    ///
    /// # Arguments
    ///
    /// * `size_limit` - The number of bytes that the Cache is allowed to hold at a given time.
    ///
    /// # Example
    ///
    /// ```
    /// use rocket_file_cache::Cache;
    /// let mut cache = Cache::new(1024 * 1024 * 30); // Create a cache that can hold 30 MB of files
    /// ```
    pub fn new(size_limit: usize) -> Cache {
        Cache {
            size_limit,
            min_file_size: 0,
            max_file_size: usize::MAX,
            priority_function: default_priority_function,
            file_map: ConcHashMap::<PathBuf, InMemoryFile, RandomState>::new(),
            access_count_map: ConcHashMap::<PathBuf, AtomicUsize, RandomState>::new(),
        }
    }

    /// Either gets the file from the cache if it exists there, gets it from the filesystem and
    /// tries to cache it, or fails to find the file and returns None.
    ///
    /// The CachedFile that is returned takes a lock out on that file in the cache.
    /// This lock will release when the CachedFile goes out of scope.
    ///
    /// # Arguments
    ///
    /// * `path` - A path that represents the path of the file in the filesystem. The path
    /// also acts as a key for the file in the cache.
    /// The path will be used to find a cached file in the cache or find a file in the filesystem if
    /// an entry in the cache doesn't exist.
    ///
    /// # Example
    ///
    /// ```
    /// #![feature(attr_literals)]
    /// #![feature(custom_attribute)]
    /// # extern crate rocket;
    /// # extern crate rocket_file_cache;
    ///
    /// # fn main() {
    /// use rocket_file_cache::{Cache, CachedFile};
    /// use std::path::{Path, PathBuf};
    /// use rocket::State;
    /// use std::sync::Arc;
    /// use std::sync::atomic::AtomicPtr;
    ///
    ///
    /// #[get("/<file..>")]
    /// fn files<'a>(file: PathBuf,  cache: State<'a, Cache> ) -> Option<CachedFile<'a>> {
    ///     let path: PathBuf = Path::new("www/").join(file).to_owned();
    ///     cache.inner().get(path)
    /// }
    /// # }
    /// ```
    pub fn get<'a, P: AsRef<Path>>(&'a self, path: P) -> Option<CachedFile<'a>> {
        trace!("{:#?}", self);
        // First, try to get the file in the cache that corresponds to the desired path.

        if self.contains_key(&path.as_ref().to_path_buf()) {
            // File is in the cache, increment the count
            self.increment_access_count(&path);
            self.update_stats(&path);

        } else {
            return self.try_insert(path).ok();
        }

        Some(CachedFile::from(self.get_from_cache(&path).unwrap()))
    }


    /// If a file has changed on disk, the cache will not automatically know that a change has occurred.
    /// Calling this function will check if the file exists, read the new file into memory,
    /// replace the old file, and update the priority score to reflect the new size of the file.
    ///
    /// # Arguments
    ///
    /// * `path` - A path that represents the path of the file in the filesystem, and key to
    /// the file in the cache.
    /// The path will be used to find the new file in the filesystem and to find the old file to replace in
    /// the cache.
    pub fn refresh<P: AsRef<Path>>(&self, path: P) -> bool {

        let mut is_ok_to_refresh: bool = false;

        // Check if the file exists in the cache
        if self.contains_key(&path.as_ref().to_path_buf()) {
            // See if the new file exists.
            let path_string: String = match path.as_ref().to_str() {
                Some(s) => String::from(s),
                None => return false,
            };
            if let Ok(metadata) = fs::metadata(path_string.as_str()) {
                if metadata.is_file() {
                    // If the entry for the old file exists
                    if self.file_map.find(&path.as_ref().to_path_buf()).is_some() {
                        is_ok_to_refresh = true;
                    }
                }
            };
        }

        if is_ok_to_refresh {
            if let Ok(new_file) = InMemoryFile::open(path.as_ref().to_path_buf()) {
                debug!("Refreshing file: {:?}", path.as_ref());
                {
                    self.file_map.remove(&path.as_ref().to_path_buf());
                    self.file_map.insert(path.as_ref().to_path_buf(), new_file);
                }

                self.update_stats(path)

            }
        }
        is_ok_to_refresh
    }

    /// Removes the file from the cache.
    /// This will not reset the access count, so the next time the file is accessed, it will be added to the cache again.
    /// The access count will have to be reset separately using `alter_access_count()`.
    ///
    /// # Arguments
    ///
    /// * `path` - A path that acts as a key to look up the file that should be removed from the cache.
    ///
    /// # Example
    ///
    /// ```
    /// use rocket_file_cache::Cache;
    /// use std::path::PathBuf;
    ///
    /// let mut cache = Cache::new(1024 * 1024 * 10);
    /// let pathbuf = PathBuf::new();
    /// cache.remove(&pathbuf);
    /// assert!(cache.contains_key(&pathbuf) == false);
    /// ```
    pub fn remove<P: AsRef<Path>>(&self, path: P) -> bool {
        if let Some(_) = self.file_map.remove(&path.as_ref().to_path_buf()){
            true
        } else {
            false
        }
    }

    /// Returns a boolean indicating if the cache has an entry corresponding to the given key.
    ///
    /// # Arguments
    ///
    /// * `path` - A path that is used as a key to look up the file.
    ///
    /// # Example
    ///
    /// ```
    /// use rocket_file_cache::Cache;
    /// use std::path::PathBuf;
    ///
    /// let mut cache = Cache::new(1024 * 1024 * 20);
    /// let pathbuf: PathBuf = PathBuf::new();
    /// cache.get(&pathbuf);
    /// assert!(cache.contains_key(&pathbuf) == false);
    /// ```
    pub fn contains_key<P: AsRef<Path>>(&self, path: P) -> bool {
        self.file_map.find(&path.as_ref().to_path_buf()).is_some()
    }


    /// Alters the access count value of one file in the access_count_map.
    /// # Arguments
    ///
    /// * `path` - The key to look up the file.
    /// * `alter_count_function` - A function that determines how to alter the access_count for the file.
    ///
    /// # Example
    ///
    /// ```
    /// use rocket_file_cache::Cache;
    /// use std::path::PathBuf;
    ///
    /// let mut cache = Cache::new(1024 * 1024 * 10);
    /// let pathbuf = PathBuf::new();
    /// cache.get(&pathbuf); // Add a file to the cache
    /// cache.remove(&pathbuf); // Removing the file will not reset its access count.
    /// cache.alter_access_count(&pathbuf, | x | { 0 }); // Set the access count to 0.
    /// ```
    ///
    pub fn alter_access_count<P: AsRef<Path>>(&self, path: P, alter_count_function: fn(&usize) -> usize) -> bool {
        let new_count: usize;
        {
            match self.access_count_map.find(&path.as_ref().to_path_buf()) {
                Some(access_count_entry) => {
                    new_count = alter_count_function(&access_count_entry.get().load(Ordering::Relaxed));
                }
                None => return false, // Can't update a file that isn't listed.
            }
        }
        {
            self.access_count_map.insert(
                path.as_ref().to_path_buf(),
                AtomicUsize::new(new_count),
            );
        }
        self.update_stats(&path);
        return true;
    }

    /// Alters the access count value of every file in the access_count_map.
    /// This is useful for manually aging-out entries in the cache.
    ///
    /// # Arguments
    ///
    /// * `alter_count_function` - A function that determines how to alter the access_count for the file.
    ///
    /// # Example
    ///
    /// ```
    /// use rocket_file_cache::Cache;
    /// use std::path::PathBuf;
    ///
    /// let mut cache = Cache::new(1024 * 1024 * 10);
    /// let pathbuf = PathBuf::new();
    /// let other_pathbuf = PathBuf::new();
    /// cache.get(&pathbuf);
    /// cache.get(&other_pathbuf);
    /// // Reduce all access counts by half,
    /// // allowing newer files to enter the cache more easily.
    /// cache.alter_all_access_counts(| x | { x / 2 });
    /// ```
    ///
    pub fn alter_all_access_counts(&self, alter_count_function: fn(&usize) -> usize) {
        let all_counts: Vec<PathBuf>;
        {
            all_counts = self.access_count_map
                .iter()
                .map(|x: (&PathBuf, &AtomicUsize)| x.0.clone())
                .collect();
        }
        for pathbuf in all_counts {
            self.alter_access_count(&pathbuf, alter_count_function);
        }

    }

    /// Gets the sum of the sizes of the files that are stored in the cache.
    ///
    /// # Example
    ///
    /// ```
    /// use rocket_file_cache::Cache;
    ///
    /// let cache = Cache::new(1024 * 1024 * 30);
    /// assert!(cache.used_bytes() == 0);
    /// ```
    pub fn used_bytes(&self) -> usize {
        self.file_map.iter().fold(
            0usize,
            |size, x| size + x.1.stats.size,
        )
    }

    /// Gets the size of the file from the file's metadata.
    /// This avoids having to read the file into memory in order to get the file size.
    fn get_file_size_from_metadata<P: AsRef<Path>>(path: P) -> Result<usize, CacheError> {
        let path_string: String = match path.as_ref().to_str() {
            Some(s) => String::from(s),
            None => return Err(CacheError::InvalidPath),
        };
        let metadata: Metadata = match fs::metadata(path_string.as_str()) {
            Ok(m) => m,
            Err(_) => return Err(CacheError::InvalidMetadata),
        };
        let size: usize = metadata.len() as usize;
        Ok(size)
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
    /// It will get the size of the file to be inserted.
    /// If will use this size to check if the file could be inserted.
    /// If it can be inserted, it reads the file into memory, stores a copy of the in-memory
    /// file behind a pointer, and constructs a CachedFile to return.
    ///
    /// If the file can't be added, it will open a NamedFile and construct a CachedFile from that,
    /// and return it.
    /// This means that it doesn't need to read the whole file into memory before reading through it
    /// again to set the response body.
    /// The lack of the need to read the whole file twice keeps performance of cache misses on par
    /// with just normally reading the file without a cache.
    ///
    ///
    /// # Arguments
    ///
    /// * `path` - The path of the file to be stored. Acts as a key for the file in the cache. Is used
    /// look up the location of the file in the filesystem if the file is not in the cache.
    ///
    ///
    fn try_insert<P: AsRef<Path>>(&self, path: P) -> Result<CachedFile, CacheError> {
        let path: PathBuf = path.as_ref().to_path_buf();
        trace!("Trying to insert file {:?}", path);

        // If the FS can read metadata for a file, then the file exists, and it should be safe to increment
        // the access_count and update.
        let size: usize = Cache::get_file_size_from_metadata(&path)?;

        // Determine how much space can still be used (represented by a negative value) or how much
        // space needs to be freed in order to make room for the new file
        let required_space_for_new_file: isize = (self.used_bytes() as isize + size as isize) - self.size_limit as isize;


        if size > self.max_file_size || size < self.min_file_size {
            self.get_file_from_fs(&path)
        } else if required_space_for_new_file < 0 && size < self.size_limit {
            self.get_file_from_fs_and_add_to_cache(&path)
        } else {
            debug!("Trying to make room for the file");

            // Because the size was gotten from the file's metadata, we know that it exists,
            // so its fine to increment the account
            self.increment_access_count(&path);

            // The access_count should have incremented since the last time this was called, so the priority must be recalculated.
            // Also, the size generally
            let new_file_priority: usize;
            {
                let default_atomic_access_count = AtomicUsize::new(1);
                let new_file_access_count: &AtomicUsize = match self.access_count_map.find(&path) {
                    Some(access_count) => &access_count.get(),
                    None => &default_atomic_access_count,
                };
                new_file_priority = (self.priority_function)(new_file_access_count.load(Ordering::Relaxed), size);
            }


            match self.make_room_for_new_file(required_space_for_new_file as usize, new_file_priority) {
                Ok(files_to_be_removed) => {
                    debug!("Made room for new file");
                    match InMemoryFile::open(path.as_path()) {
                        Ok(file) => {

                            // We have read a new file into memory, it is safe to
                            // remove the old files.
                            for file_key in files_to_be_removed {
                                // The file was accessed with this key earlier when sorting priorities.
                                // Unwrapping be safe.
                                let _ = self.file_map.remove(&file_key).expect(
                                    "Because the file was just accessed, it should be safe to remove it from the map.",
                                );
                            }

                            self.file_map.insert(path.clone(), file);
                            self.update_stats(&path);

                            let cached_file: NamedInMemoryFile = NamedInMemoryFile::new(path.clone(), self.file_map.find(&path).unwrap());

                            return Ok(CachedFile::from(cached_file));
                        }
                        Err(_) => return Err(CacheError::InvalidPath),
                    }
                }
                Err(_) => {
                    debug!("The file does not have enough priority or is too large to be accepted into the cache.");
                    // The new file would not be accepted by the cache, so instead of reading the whole file
                    // into memory, and then copying it yet again when it is attached to the body of the
                    // response, use a NamedFile instead.
                    match NamedFile::open(path.clone()) {
                        Ok(named_file) => Ok(CachedFile::from(named_file)),
                        Err(_) => Err(CacheError::InvalidPath),
                    }
                }
            }
        }
    }

    /// Gets a file from the filesystem and converts it to a CachedFile.
    ///
    /// This should be used when the cache knows that the new file won't make it into the cache.
    fn get_file_from_fs<P: AsRef<Path>>(&self, path: P) ->Result<CachedFile, CacheError> {
        debug!("File does not fit size constraints of the cache.");
        match NamedFile::open(path.as_ref().to_path_buf()) {
            Ok(named_file) => {
                self.increment_access_count(path);
                return Ok(CachedFile::from(named_file));
            }
            Err(_) => return Err(CacheError::InvalidPath),
        }
    }

    /// Reads a file from the filesystem into memory and stores it in the cache.
    ///
    /// This is the slowest operation the cache can perform, slower than just getting the file.
    /// It should only be used when the cache decides to store the file.
    fn get_file_from_fs_and_add_to_cache<P: AsRef<Path>>(&self, path: P) ->Result<CachedFile, CacheError> {
        debug!("Cache has room for the file.");
        match InMemoryFile::open(&path) {
            Ok(file) => {
                self.file_map.insert(path.as_ref().to_path_buf(), file);

                self.increment_access_count(&path);
                self.update_stats(&path);

                let cached_file = NamedInMemoryFile::new(path.as_ref().to_path_buf(), self.file_map.find(path.as_ref()).unwrap());

                return Ok(CachedFile::from(cached_file));
            }
            Err(_) => return Err(CacheError::InvalidPath),
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
    fn make_room_for_new_file(&self, required_space: usize, new_file_priority: usize) -> Result<Vec<PathBuf>, CacheError> {
        let mut possibly_freed_space: usize = 0;
        let mut priority_score_to_free: usize = 0;
        let mut file_paths_to_remove: Vec<PathBuf> = vec![];

        let mut stats: Vec<(PathBuf, FileStats)> = self.sorted_priorities();
        while possibly_freed_space < required_space {
            // pop the priority group with the lowest priority off of the vector
            match stats.pop() {
                Some(lowest) => {
                    let (lowest_key, lowest_stats) = lowest;

                    possibly_freed_space += lowest_stats.size;
                    priority_score_to_free += lowest_stats.priority;
                    file_paths_to_remove.push(lowest_key.clone());

                    // Check if total priority to free is greater than the new file's priority,
                    // If it is, then don't free the files, as they in aggregate, are more important
                    // than the new file.
                    if priority_score_to_free > new_file_priority {
                        return Err(CacheError::NewPriorityIsNotHighEnough);
                    }
                }
                None => return Err(CacheError::NoMoreFilesToRemove),
            };
        }
        Ok(file_paths_to_remove)

    }

    ///Helper function that gets the file from the cache if it exists there.
    fn get_from_cache<P: AsRef<Path>>(&self, path: P) -> Option<NamedInMemoryFile> {
        match self.file_map.find(&path.as_ref().to_path_buf()) {
            Some(in_memory_file) => {
                Some(NamedInMemoryFile::new(
                    path.as_ref().to_path_buf(),
                    in_memory_file,
                ))
            }
            None => None, // File not found
        }

    }

    /// Helper function for incrementing the access count for a given file name.
    ///
    /// This should only be used in cases where the file is known to exist, to avoid bloating the access count map with useless values.
    fn increment_access_count<P: AsRef<Path>>(&self, path: P) {
        self.access_count_map.upsert(
            path.as_ref().to_path_buf(),
            AtomicUsize::new(1), // insert 1 if nothing at key
            &|access_count| {
                let new_access_count: usize = match usize::checked_add(access_count.load(Ordering::Relaxed), 1) {
                    Some(v) => v,
                    None => usize::MAX
                };
                access_count.store( new_access_count, Ordering::Relaxed)
            }, // increment by 1 if key found
        );
    }


    /// Update the stats associated with this file.
    fn update_stats<P: AsRef<Path>>(&self, path: P) {

        let default_atomic_access_count = AtomicUsize::new(1);
        let access_count: &AtomicUsize = match self.access_count_map.find(&path.as_ref().to_path_buf()) {
            Some(access_count) => access_count.get(),
            None => &default_atomic_access_count,
        };

        self.file_map.upsert(
            // Key
            path.as_ref().to_path_buf(),
            // Default Value
            InMemoryFile {
                bytes: Vec::new(),
                stats: FileStats {
                    size: 0,
                    access_count: 0,
                    priority: 0,
                },
            },
            // Update Function
            &|file_entry| {
                // If the size is initialized to 0, then try to get the actual size from the filesystem
                if file_entry.stats.size == 0 {
                    file_entry.stats.size = Cache::get_file_size_from_metadata(&path.as_ref().to_path_buf()).unwrap_or(0);
                }
                file_entry.stats.access_count = access_count.load(Ordering::Relaxed);
                file_entry.stats.priority = (self.priority_function)(file_entry.stats.access_count, file_entry.stats.size); // update the priority score.
            },
        );


    }





    /// Gets a vector of tuples containing the Path, priority score, and size in bytes of all items
    /// in the file_map.
    ///
    /// The vector is sorted from highest to lowest priority.
    /// This allows the assumption that the last element to be popped from the vector will have the
    /// lowest priority, and therefore is the most eligible candidate for elimination from the
    /// cache.
    ///
    fn sorted_priorities(&self) -> Vec<(PathBuf, FileStats)> {

        let mut priorities: Vec<(PathBuf, FileStats)> = self.file_map
            .iter()
            .map(|x| (x.0.clone(), x.1.stats.clone()))
            .collect();

        // Sort the priorities from highest priority to lowest, so when they are pop()ed later,
        // the last element will have the lowest priority.
        priorities.sort_by(|l, r| r.1.priority.cmp(&l.1.priority));
        priorities
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
    use in_memory_file::InMemoryFile;
    use concurrent_hashmap::Accessor;
    use std::sync::Arc;


    const MEG1: usize = 1024 * 1024;
    const MEG2: usize = MEG1 * 2;
    const MEG5: usize = MEG1 * 5;
    const MEG10: usize = MEG1 * 10;

    const DIR_TEST: &'static str = "test1";
    const FILE_MEG1: &'static str = "meg1.txt";
    const FILE_MEG2: &'static str = "meg2.txt";
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


    // Standardize the way a file is used in these tests.
    impl<'a> CachedFile<'a> {
        fn dummy_write(self) {
            match self {
                CachedFile::Cached(cached_file) => unsafe {
                    let file: *const Accessor<'a, PathBuf, InMemoryFile> = Arc::into_raw(cached_file.file);
                    let _ = (*file).get().bytes.clone();
                    let _ = Arc::from_raw(file); // To prevent a memory leak, an Arc needs to be reconstructed from the raw pointer.
                },
                CachedFile::FileSystem(mut named_file) => {
                    let mut v: Vec<u8> = Vec::new();
                    let _ = named_file.read_to_end(&mut v).unwrap();
                }
            }
        }

        fn get_in_memory_file(self) -> NamedInMemoryFile<'a> {
            match self {
                CachedFile::Cached(n) => n,
                CachedFile::FileSystem(_) => panic!("tried to get cached file for named file"),
            }
        }
    }

    #[bench]
    fn cache_get_10mb(b: &mut Bencher) {
        let cache: Cache = Cache::new(MEG1 * 20); //Cache can hold 20Mb
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_10m = create_test_file(&temp_dir, MEG10, FILE_MEG10);
        cache.get(&path_10m); // add the 10 mb file to the cache

        b.iter(|| {
            let cached_file = cache.get(&path_10m).unwrap();
            cached_file.dummy_write()
        });
    }

    #[bench]
    fn cache_miss_10mb(b: &mut Bencher) {
        let cache: Cache = Cache::new(0);
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_10m = create_test_file(&temp_dir, MEG10, FILE_MEG10);

        b.iter(|| {
            let cached_file = cache.get(&path_10m).unwrap();
            cached_file.dummy_write()
        });
    }

    #[bench]
    fn named_file_read_10mb(b: &mut Bencher) {
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_10m = create_test_file(&temp_dir, MEG10, FILE_MEG10);
        b.iter(|| {
            let named_file = CachedFile::from(NamedFile::open(&path_10m).unwrap());
            named_file.dummy_write()
        });
    }

    #[bench]
    fn cache_get_1mb(b: &mut Bencher) {
        let cache: Cache = Cache::new(MEG1 * 20); //Cache can hold 20Mb
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_1m = create_test_file(&temp_dir, MEG1, FILE_MEG1);
        cache.get(&path_1m); // add the 10 mb file to the cache

        b.iter(|| {
            let cached_file = cache.get(&path_1m).unwrap();
            cached_file.dummy_write()
        });
    }

    #[bench]
    fn cache_miss_1mb(b: &mut Bencher) {
        let cache: Cache = Cache::new(0);
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_1m = create_test_file(&temp_dir, MEG1, FILE_MEG1);

        b.iter(|| {
            let cached_file = cache.get(&path_1m).unwrap();
            cached_file.dummy_write()
        });
    }

    #[bench]
    fn named_file_read_1mb(b: &mut Bencher) {
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_1m = create_test_file(&temp_dir, MEG1, FILE_MEG1);

        b.iter(|| {
            let named_file = CachedFile::from(NamedFile::open(&path_1m).unwrap());
            named_file.dummy_write()
        });
    }



    #[bench]
    fn cache_get_5mb(b: &mut Bencher) {
        let cache: Cache = Cache::new(MEG1 * 20); //Cache can hold 20Mb
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_5m = create_test_file(&temp_dir, MEG5, FILE_MEG5);
        cache.get(&path_5m); // add the 10 mb file to the cache

        b.iter(|| {
            let cached_file = cache.get(&path_5m).unwrap();
            cached_file.dummy_write()
        });
    }

    #[bench]
    fn cache_miss_5mb(b: &mut Bencher) {
        let cache: Cache = Cache::new(0);
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_5m = create_test_file(&temp_dir, MEG5, FILE_MEG5);

        b.iter(|| {
            let cached_file = cache.get(&path_5m).unwrap();
            cached_file.dummy_write()
        });
    }

    #[bench]
    fn named_file_read_5mb(b: &mut Bencher) {
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_5m = create_test_file(&temp_dir, MEG5, FILE_MEG5);

        b.iter(|| {
            let named_file = CachedFile::from(NamedFile::open(&path_5m).unwrap());
            named_file.dummy_write()
        });
    }

    // Constant time access regardless of size.
    #[bench]
    fn cache_get_1mb_from_1000_entry_cache(b: &mut Bencher) {
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_1m = create_test_file(&temp_dir, MEG1, FILE_MEG1);
        let cache: Cache = Cache::new(MEG1 * 3); //Cache can hold 3Mb
        cache.get(&path_1m); // add the file to the cache

        // Add 1024 1kib files to the cache.
        for i in 0..1024 {
            let path = create_test_file(&temp_dir, 1024, format!("{}_1kib.txt", i).as_str());
            cache.get(&path);
        }
        // make sure that the file has a high priority.
        cache.alter_all_access_counts(|x| x + 1 * 100000);

        assert_eq!(cache.used_bytes(), MEG1 * 2);

        b.iter(|| {
            let cached_file = cache.get(&path_1m).unwrap();
            cached_file.dummy_write()
        });
    }

    // There is a penalty for missing the cache.
    #[bench]
    fn cache_miss_1mb_from_1000_entry_cache(b: &mut Bencher) {
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_1m = create_test_file(&temp_dir, MEG1, FILE_MEG1);
        let cache: Cache = Cache::new(MEG1); //Cache can hold 1Mb

        // Add 1024 1kib files to the cache.
        for i in 0..1024 {
            let path = create_test_file(&temp_dir, 1024, format!("{}_1kib.txt", i).as_str());
            cache.get(&path);
        }
        // make sure that the file has a high priority.
        cache.alter_all_access_counts(|x| x + 1 * 100000);
        //        println!("{:#?}", cache);

        b.iter(|| {
            let cached_file = cache.get(&path_1m).unwrap();
            // Mimic what is done when the response body is set.
            cached_file.dummy_write()
        });
    }

    // This is pretty much a worst-case scenario, where every file would have to be removed to make room for the new file.
    // There is a penalty for missing the cache.
    #[bench]
    fn cache_miss_5mb_from_1000_entry_cache(b: &mut Bencher) {
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_5m = create_test_file(&temp_dir, MEG5, FILE_MEG1);
        let cache: Cache = Cache::new(MEG5); //Cache can hold 1Mb

        // Add 1024 5kib files to the cache.
        for i in 0..1024 {
            let path = create_test_file(&temp_dir, 1024 * 5, format!("{}_5kib.txt", i).as_str());
            cache.get(&path);
        }
        // make sure that the file has a high priority.
        cache.alter_all_access_counts(|x| x + 1 * 100000);

        b.iter(|| {
            let cached_file: CachedFile = cache.get(&path_5m).unwrap();
            // Mimic what is done when the response body is set.
            cached_file.dummy_write()
        });
    }


    #[bench]
    fn in_memory_file_read_10mb(b: &mut Bencher) {
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_10m = create_test_file(&temp_dir, MEG10, FILE_MEG10);

        b.iter(|| {
            let in_memory_file = Arc::new(InMemoryFile::open(path_10m.clone()).unwrap());
            let file: *const InMemoryFile = Arc::into_raw(in_memory_file);
            unsafe {
                let _ = (*file).bytes.clone();
                let _ = Arc::from_raw(file);
            }
        });
    }


    #[test]
    fn file_exceeds_size_limit() {
        let cache: Cache = Cache::new(MEG1 * 8); // Cache can hold only 8Mb
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_10m = create_test_file(&temp_dir, MEG10, FILE_MEG10);

        let named_file = NamedFile::open(path_10m.clone()).unwrap();

        // expect the cache to get the item from the FS.
        assert_eq!(cache.try_insert(path_10m), Ok(CachedFile::from(named_file)));
    }


    #[test]
    fn file_replaces_other_file() {
        let temp_dir = TempDir::new(DIR_TEST).unwrap();

        let path_1m = create_test_file(&temp_dir, MEG1, FILE_MEG1);
        let path_5m = create_test_file(&temp_dir, MEG5, FILE_MEG5);

        let named_file_1m = NamedFile::open(path_1m.clone()).unwrap();
        let named_file_1m_2 = NamedFile::open(path_1m.clone()).unwrap();


        let mut imf_5m = InMemoryFile::open(path_5m.clone()).unwrap();
        let mut imf_1m = InMemoryFile::open(path_1m.clone()).unwrap();

        // set expected stats for 5m
        imf_5m.stats.access_count = 1;
        imf_5m.stats.priority = 2289;


        let cache: Cache = Cache::new(5500000); //Cache can hold only 5.5Mib

        println!("0:\n{:#?}", cache);

        assert_eq!(
            cache
                .try_insert(path_5m.clone())
                .unwrap()
                .get_in_memory_file()
                .file
                .as_ref()
                .get(),
            &imf_5m
        );
        println!("1:\n{:#?}", cache);
        assert_eq!(
            cache.try_insert(path_1m.clone()),
            Ok(CachedFile::from(named_file_1m))
        );
        println!("2:\n{:#?}", cache);
        assert_eq!(
            cache.try_insert(path_1m.clone()),
            Ok(CachedFile::from(named_file_1m_2))
        );
        println!("3:\n{:#?}", cache);

        // set the expected stats for 1m
        imf_1m.stats.access_count = 3;
        imf_1m.stats.priority = 3072;

        assert_eq!(
            cache
                .try_insert(path_1m.clone())
                .unwrap()
                .get_in_memory_file()
                .file
                .as_ref()
                .get(),
            &imf_1m
        );
        println!("4:\n{:#?}", cache);
    }




    #[test]
    fn new_file_replaces_lowest_priority_file() {
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_1m = create_test_file(&temp_dir, MEG1, FILE_MEG1);
        let path_2m = create_test_file(&temp_dir, MEG2, FILE_MEG2);
        let path_5m = create_test_file(&temp_dir, MEG5, FILE_MEG5);


        #[allow(unused_variables)]
        let named_file_1m = NamedFile::open(path_1m.clone()).unwrap();

        let cache: Cache = Cache::new(MEG1 * 7 + 2000);

        // TODO change these to useful assert_eq!s
        println!("1:\n{:#?}", cache);
        assert!(cache.get(&path_5m).is_some());

        println!("2:\n{:#?}", cache);
        assert!(cache.get(&path_2m).is_some());

        println!("3:\n{:#?}", cache);
        assert!(cache.get(&path_1m).is_some());
        println!("4:\n{:#?}", cache);
        // The cache will now accept the 1 meg file because (sqrt(2)_size * 1_access) for the old
        // file is less than (sqrt(1)_size * 2_access) for the new file.
        assert!(cache.get(&path_1m).is_some());

        println!("5:\n{:#?}", cache);

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

        drop(cache);
    }




    #[test]
    fn remove_file() {
        let cache: Cache = Cache::new(MEG1 * 10);
        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_5m = create_test_file(&temp_dir, MEG5, FILE_MEG5);

        let mut imf: InMemoryFile = InMemoryFile::open(path_5m.clone()).unwrap();

        // Set the expected values for the stats in IMF.
        imf.stats.priority = 2289;
        imf.stats.access_count = 1;

        // expect the cache to get the item from the FS.
        assert_eq!(
            cache
                .get(&path_5m)
                .unwrap()
                .get_in_memory_file()
                .file
                .as_ref()
                .get(),
            &imf
        );

        cache.remove(&path_5m);

        assert_eq!(cache.contains_key(&path_5m.clone()), false);
    }

    #[test]
    fn refresh_file() {
        let cache: Cache = Cache::new(MEG1 * 10);

        let temp_dir = TempDir::new(DIR_TEST).unwrap();
        let path_5m = create_test_file(&temp_dir, MEG5, FILE_MEG5);


        //        assert_eq!(
        //            cache.get(&path_5m).unwrap(),
        //            Some(CachedFile::from(cached_file))
        //        );

        assert_eq!(
            match cache.get(&path_5m).unwrap() {
                CachedFile::Cached(c) => c.file.get().stats.size,
                CachedFile::FileSystem(_) => unreachable!(),
            },
            MEG5
        );

        let path_of_file_with_10mb_but_path_name_5m = create_test_file(&temp_dir, MEG10, FILE_MEG5);


        cache.refresh(&path_5m);

        assert_eq!(
            match cache.get(&path_of_file_with_10mb_but_path_name_5m).unwrap() {
                CachedFile::Cached(c) => c.file.get().stats.size,
                CachedFile::FileSystem(_) => unreachable!(),
            },
            MEG10
        );

        drop(cache);
    }

}
