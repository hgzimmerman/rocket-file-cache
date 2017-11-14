use std::usize;


/// A closure that matches this type signature can be specified at cache instantiation to define how the cache will invalidate or add items to itself.
///
/// The function type that is used to determine how to add files to the cache.
/// The first term will be assigned the access count of the file in question, while the second term will be assigned the size (in bytes) of the file in question.
/// The result will represent the priority of the file to remain in, or be added to the cache.
/// The files with the largest priorities will be kept in the cache.
pub type PriorityFunction = fn(usize, usize) -> usize;


/// The default priority function used for determining if a file should be in the cache.
///
/// This function takes the square root of the size of the file times the number of times it has been accessed.
/// This should give some priority to bigger files, while still allowing some smaller files to enter the cache.
pub fn default_priority_function(access_count: usize, size: usize) -> usize {
 ((size as f64).sqrt() as usize) * access_count
}

/// Priority is calculated as the size times the access count.
pub fn normal_priority_function(access_count: usize, size: usize ) -> usize {
    size * access_count
}

/// This priority function will value files in the cache based solely on the number of times the file was accessed.
pub fn access_priority_function(access_count: usize, _: usize) -> usize {
    access_count
}


/// Favor small files without respect to the number of times file was accessed.
///
/// The smaller the file, the higher priority it will have.
/// Does not take into account the number of accesses the file has.
pub fn small_files_priority_function(_: usize, size: usize) -> usize {
    if size == 0 {
        return 0 // don't give any priority to completely empty files.
    }
    usize::MAX / size
}

/// Favor small files with respect to the number of times file was accessed.
///
/// The smaller the file, the higher priority it will have.
/// Does take into account the number of accesses the file has.
pub fn small_files_access_priority_function(access_count: usize, size: usize) -> usize {
    if size == 0 {
        return 0 // don't give any priority to completely empty files.
    }
    (usize::MAX / size) * access_count
}
