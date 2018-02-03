use cache::{Cache, AgeOut};

use priority_function::default_priority_function;
use std::usize;

use concurrent_hashmap::{ConcHashMap, Options};
use std::collections::hash_map::RandomState;
use std::sync::atomic::AtomicUsize;



/// Error types that can be encountered when a cache is built.
#[derive(Debug, PartialEq)]
pub enum CacheBuildError {
    MinFileSizeIsLargerThanMaxFileSize,
}

/// A builder for Caches.
#[derive(Debug)]
pub struct CacheBuilder {
    size_limit: Option<usize>,
    accesses_per_refresh: Option<usize>,
    concurrency: Option<u16>,
    priority_function: Option<fn(usize, usize) -> usize>,
    min_file_size: Option<usize>,
    max_file_size: Option<usize>,
    age_out: Option<AgeOut>
}


impl CacheBuilder {

    /// Create a new CacheBuilder.
    ///

    pub fn new() -> CacheBuilder {
        CacheBuilder {
            size_limit: None,
            accesses_per_refresh: None,
            concurrency: None,
            priority_function: None,
            min_file_size: None,
            max_file_size: None,
            age_out: None,
        }
    }

    pub fn age_out<'a>(&'a mut self, accesses_limit: usize, age_out_function: fn(&AtomicUsize)) {
        self.age_out = Some(AgeOut {
            accesses_limit,
            access_count: AtomicUsize::from(0),
            age_out_function,
        });
    }

    /// Sets the maximum number of bytes (as they exist in the FS) that the cache can hold.
    /// The cache will take up more space in memory due to the backing concurrent HashMap it uses.
    /// The memory overhead can be controlled by setting the concurrency parameter.
    ///
    /// # Arguments
    /// * size_limit - The number of bytes the cache will be able to hold.
    ///
    pub fn size_limit<'a>(&'a mut self, size_limit: usize) -> &mut Self {
        self.size_limit = Some(size_limit);
        self
    }

    /// Sets the concurrency setting of the concurrent hashmap backing the cache.
    /// A higher concurrency setting allows more threads to access the hashmap at the expense of more memory use.
    /// The default is 16.
    pub fn concurrency<'a>(&'a mut self, concurrency: u16) -> &mut Self {
        self.concurrency = Some(concurrency);
        self
    }


    /// Sets the number of times a file can be accessed from the cache before it will be refreshed from the disk.
    /// By providing 1000, that will instruct the cache to refresh the file every 1000 times its accessed.
    /// By default, the cache will not refresh the file.
    ///
    /// This should be useful if you anticipate bitrot for the cache contents in RAM, as it will
    /// refresh the file from the FileSystem, meaning that if there is an error in the cached data,
    /// it will only be served for an average of n/2 accesses before the automatic refresh replaces it
    /// with an assumed correct copy.
    /// Using ECC RAM should mitigate the possibility of bitrot.
    ///
    /// # Panics
    /// This function will panic if a 0 or 1 are supplied.
    /// Something modulo 0 (used when calculating if the file will refresh) will result in an error later.
    /// The cache would try to refresh on every access if 1 was used as the value, which is less
    /// efficient than just accessing the files directly.
    ///
    pub fn accesses_per_refresh<'a>(&'a mut self, accesses: usize) -> &mut Self {
        if accesses < 2 {
            panic!("Incorrectly configured access_per_refresh rate. Values of 0 or 1 are not allowed.");
        } else {
            self.accesses_per_refresh = Some(accesses);
        }
        self
    }


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
    /// # Example
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
    pub fn build(self) -> Result<Cache, CacheBuildError> {

        let size_limit: usize = match self.size_limit {
            Some(s) => s,
            None => {
                warn!("Size for cache not configured. This may lead to the cache using more memory than necessary.");
                usize::MAX
            }
        };

        let priority_function = match self.priority_function {
            Some(pf) => pf,
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


        Ok(Cache {
            size_limit: size_limit,
            min_file_size,
            max_file_size,
            priority_function,
            age_out: self.age_out,
            accesses_per_refresh: self.accesses_per_refresh,
            file_map: ConcHashMap::with_options(options_files_map),
            access_count_map: ConcHashMap::with_options(options_access_map),
        })

    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn min_greater_than_max() {

        let e: CacheBuildError = CacheBuilder::new()
            .min_file_size(1024 * 1024 * 5)
            .max_file_size(1024 * 1024 * 4)
            .build()
            .unwrap_err();
        assert_eq!(CacheBuildError::MinFileSizeIsLargerThanMaxFileSize, e);
    }

    #[test]
    fn all_options_used_in_build() {
        let _: Cache = CacheBuilder::new()
            .size_limit(1024 * 1024 * 20)
            .priority_function(|access_count: usize, size: usize| access_count * size)
            .max_file_size(1024 * 1024 * 10)
            .min_file_size(1024 * 10)
            .concurrency(20)
            .accesses_per_refresh(1000)
            .build()
            .unwrap();
    }

}
