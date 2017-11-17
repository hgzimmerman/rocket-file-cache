use cache::Cache;

use std::collections::HashMap;
use priority_function::{PriorityFunction, default_priority_function};
use std::usize;
use std::sync::Mutex;


/// Error types that can be encountered when a cache is built.
#[derive(Debug, PartialEq)]
pub enum CacheBuildError {
    SizeLimitNotSet,
    MinFileSizeIsLargerThanMaxFileSize
}

/// A builder for Caches.
#[derive(Debug)]
pub struct CacheBuilder {
    size_limit: Option<usize>,
    priority_function: Option<PriorityFunction>,
    min_file_size: Option<usize>,
    max_file_size: Option<usize>
}



impl CacheBuilder {
    /// Create a new CacheBuilder.
    pub fn new() -> CacheBuilder {
        CacheBuilder {
            size_limit: None,
            priority_function: None,
            min_file_size: None,
            max_file_size: None
        }
    }

    /// Mandatory parameter.
    /// Sets the maximum number of bytes the cache can hold.
    pub fn size_limit_bytes<'a>(&'a mut self, size_limit_bytes: usize) -> &mut Self {
        self.size_limit = Some(size_limit_bytes);
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
    ///
    /// ```
    /// use rocket_file_cache::Cache;
    /// use rocket_file_cache::CacheBuilder;
    /// let cache: Cache = CacheBuilder::new()
    ///     .size_limit_bytes(1024 * 1024 * 50) // 50 MB cache
    ///     .priority_function(|access_count, size| {
    ///         access_count * access_count * size
    ///     })
    ///     .build()
    ///     .unwrap();
    /// ```
    pub fn priority_function<'a>(&'a mut self, priority_function: PriorityFunction) -> &mut Self {
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
    /// let cache: Cache = CacheBuilder::new()
    ///     .size_limit_bytes(1024 * 1024 * 50) // 50 MB cache
    ///     .min_file_size(1024 * 4) // Don't store files smaller than 4 KB
    ///     .max_file_size(1024 * 1024 * 6) // Don't store files larger than 6 MB
    ///     .build()
    ///     .unwrap();
    /// ```
    pub fn build(&self) -> Result<Cache, CacheBuildError> {
        let size_limit = match self.size_limit {
            Some(s) => s,
            None => return Err(CacheBuildError::SizeLimitNotSet)
        };

        let priority_function: PriorityFunction = match self.priority_function {
            Some(p) => p,
            None => default_priority_function
        };

        if let Some(min_file_size) = self.min_file_size {
            if let Some(max_file_size) = self.max_file_size {
                if min_file_size > max_file_size {
                    return Err(CacheBuildError::MinFileSizeIsLargerThanMaxFileSize)
                }
            }
        }

        let min_file_size: usize = match self.min_file_size {
            Some(min) => min,
            None => 0
        };

        let max_file_size: usize = match self.max_file_size {
            Some(max) => max,
            None => usize::MAX
        };

        Ok(
            Cache {
                size_limit,
                min_file_size,
                max_file_size,
                priority_function,
                file_map: HashMap::new(),
                file_stats_map: Mutex::new(HashMap::new()),
                access_count_map: HashMap::new()
            }
        )

    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path};

    #[test]
    fn no_size_error(){
        assert_eq!(CacheBuildError::SizeLimitNotSet, CacheBuilder::new().build().unwrap_err());
    }
    #[test]
    fn min_greater_than_max() {

        let e: CacheBuildError = CacheBuilder::new()
            .size_limit_bytes(1024 * 1024 * 10)
            .min_file_size(1024 * 1024 * 5)
            .max_file_size(1024 * 1024 * 4)
            .build()
            .unwrap_err();
        assert_eq!(CacheBuildError::MinFileSizeIsLargerThanMaxFileSize, e);
    }

    #[test]
    fn can_build() {
        let _: Cache = CacheBuilder::new()
            .size_limit_bytes(1024 * 1024 * 20)
            .priority_function(|access_count: usize, size: usize| {
                access_count * size
            })
            .max_file_size(1024 * 1024 * 10)
            .min_file_size(1024 * 10)
            .build().unwrap();
    }

}
