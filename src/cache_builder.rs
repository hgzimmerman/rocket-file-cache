use cache::Cache;
use super::PriorityFunction;

use std::collections::HashMap;

pub enum CacheBuildError {
    SizeLimitNotSet,
    MinFileSizeIsLargerThanMaxFileSize
}

pub struct CacheBuilder {
    size_limit: Option<usize>,
    priority_function: Option<PriorityFunction>,
    min_file_size: Option<usize>,
    max_file_size: Option<usize>
}


impl CacheBuilder {
    pub fn new() -> CacheBuilder {
        CacheBuilder {
            size_limit: None,
            priority_function: None,
            min_file_size: None,
            max_file_size: None
        }
    }

    /// Mandatory parameter, must be set.
    /// Sets the maximum number of bytes the cache can hold.
    pub fn size_limit_bytes(&mut self, size_limit_bytes: usize) {
        self.size_limit = Some(size_limit_bytes);
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
    pub fn priority_function(&mut self, priority_function: PriorityFunction) {
        self.priority_function = Some(priority_function);
    }

    // TODO, implement min and max file sizes

    pub fn build(self) -> Result<Cache, CacheBuildError> {
        let size_limit = match self.size_limit {
            Some(s) => s,
            None => return Err(CacheBuildError::SizeLimitNotSet)
        };

        let priority_function: PriorityFunction = match self.priority_function {
            Some(p) => p,
            None => Cache::DEFAULT_PRIORITY_FUNCTION
        };

        if let Some(min_file_size) = self.min_file_size {
            if let Some(max_file_size) = self.max_file_size {
                if min_file_size > max_file_size {
                    return Err(CacheBuildError::MinFileSizeIsLargerThanMaxFileSize)
                }
            }
        }

        Ok(
            Cache {
                size_limit,
                priority_function,
                file_map: HashMap::new(),
                access_count_map: HashMap::new()
            }
        )

    }
}
