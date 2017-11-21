use cache::Cache;

use priority_function::default_priority_function;
use std::usize;

use concurrent_hashmap::{ConcHashMap, Options};
use std::collections::hash_map::RandomState;



/// Error types that can be encountered when a cache is built.
#[derive(Debug, PartialEq)]
pub enum CacheBuildError {
    MinFileSizeIsLargerThanMaxFileSize,
}

/// A builder for Caches.
#[derive(Debug)]
pub struct CacheBuilder {
    size_limit: usize,
    concurrency: Option<u16>,
    capacity: Option<usize>,
    priority_function: Option<fn(usize, usize) -> usize>,
    min_file_size: Option<usize>,
    max_file_size: Option<usize>,
}



impl CacheBuilder {
    /// Create a new CacheBuilder.
    pub fn new(size_limit: usize) -> CacheBuilder {
        CacheBuilder {
            size_limit,
            concurrency: None,
            capacity: None,
            priority_function: None,
            min_file_size: None,
            max_file_size: None,
        }
    }

    /// Sets the concurrency setting of the concurrent hashmap backing the cache.
    /// The default is 16
    pub fn concurrency<'a>(&'a mut self, concurrency: u16) -> &mut Self {
        self.concurrency = Some(concurrency);
        self
    }

//    // TODO, make this public possibly? Or delete? I don't think that this information is relevant to the use of the cache.
//    /// Sets the number of elements that should be preallocated for the concurrent hashmap backing the cache.
//    ///
//    /// The concurrent hashmap will grow to store more than the preallocated amount.
//    /// The default is 0.
//    fn initial_capacity<'a>(&'a mut self, capacity: usize) -> &mut Self {
//        self.capacity = Some(capacity);
//        self
//    }

    /// Override the default priority function used for determining if the cache should hold a file.
    /// By default a score is calculated using the square root of the size of a file, times the number
    /// of times it was accessed.
    /// Files with higher priority scores will be kept in the cache when files with lower scores are
    /// added.
    /// If there isn't room in the cache for two files, the one with the lower score will be removed /
    /// won't be added.
    ///
    /// The priority function should be kept simple, as it is calculated on every file in the cache
    /// every time a new file is attempted to be added.
    ///
    ///
    /// ```
    /// use rocket_file_cache::Cache;
    /// use rocket_file_cache::CacheBuilder;
    /// let cache: Cache = CacheBuilder::new(1024 * 1024 * 50) // 50 MB cache
    ///     .priority_function(|access_count, size| {
    ///         access_count * access_count * size
    ///     })
    ///     .build()
    ///     .unwrap();
    /// ```
    pub fn priority_function<'a>(&'a mut self, priority_function: fn(usize, usize) -> usize) -> &mut Self {
        self.priority_function = Some(priority_function);
        self
    }

    /// Set the minimum size in bytes for files that can be stored in the cache
    pub fn min_file_size<'a>(&'a mut self, min_size: usize) -> &mut Self {
        self.min_file_size = Some(min_size);
        self
    }

    /// Set the maximum size in bytes for files that can be stored in the cache
    pub fn max_file_size<'a>(&'a mut self, max_size: usize) -> &mut Self {
        self.max_file_size = Some(max_size);
        self
    }

    /// Finalize the cache.
    ///
    /// # Example
    ///
    /// ```
    /// use rocket_file_cache::Cache;
    /// use rocket_file_cache::CacheBuilder;
    ///
    /// let cache: Cache = CacheBuilder::new(1024 * 1024 * 50) // 50 MB cache
    ///     .min_file_size(1024 * 4) // Don't store files smaller than 4 KB
    ///     .max_file_size(1024 * 1024 * 6) // Don't store files larger than 6 MB
    ///     .build()
    ///     .unwrap();
    /// ```
    pub fn build(&self) -> Result<Cache, CacheBuildError> {

        let priority_function = match self.priority_function {
            Some(p) => p,
            None => default_priority_function,
        };

        if let Some(min_file_size) = self.min_file_size {
            if let Some(max_file_size) = self.max_file_size {
                if min_file_size > max_file_size {
                    return Err(CacheBuildError::MinFileSizeIsLargerThanMaxFileSize);
                }
            }
        }

        let min_file_size: usize = match self.min_file_size {
            Some(min) => min,
            None => 0,
        };

        let max_file_size: usize = match self.max_file_size {
            Some(max) => max,
            None => usize::MAX,
        };



        let mut options_files_map: Options<RandomState> = Options::default();
        let mut options_access_map: Options<RandomState> = Options::default();

        if let Some(conc) = self.concurrency {
            options_files_map.concurrency = conc;
            options_access_map.concurrency = conc;
        }

        if let Some(capacity) = self.capacity {
            options_files_map.capacity = capacity;
            options_access_map.capacity = capacity;
        }


        Ok(Cache {
            size_limit: self.size_limit,
            min_file_size,
            max_file_size,
            priority_function,
            file_map: ConcHashMap::with_options(options_files_map),
            access_count_map: ConcHashMap::with_options(options_access_map)
        })

    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn min_greater_than_max() {

        let e: CacheBuildError = CacheBuilder::new(1024 * 1024 * 10)
            .min_file_size(1024 * 1024 * 5)
            .max_file_size(1024 * 1024 * 4)
            .build()
            .unwrap_err();
        assert_eq!(CacheBuildError::MinFileSizeIsLargerThanMaxFileSize, e);
    }

    #[test]
    fn all_options_used_in_build() {
        let _: Cache = CacheBuilder::new(1024 * 1024 * 20)
            .priority_function(|access_count: usize, size: usize| access_count * size)
            .max_file_size(1024 * 1024 * 10)
            .min_file_size(1024 * 10)
            .concurrency(20)
            .build()
            .unwrap();
    }

}
