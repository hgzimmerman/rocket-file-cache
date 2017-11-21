use std::usize;

/// The default priority function used for determining if a file should be in the cache.
///
/// This function takes the square root of the size of the file times the number of times it has been accessed.
/// This should give some priority to bigger files, while still allowing some smaller files to enter the cache.
pub fn default_priority_function(access_count: usize, size: usize) -> usize {
    match usize::checked_mul(
        ((size as f64).sqrt() as usize),
        access_count
    ) {
        Some(v) => v,
        None => usize::MAX
    }
}

/// Priority is calculated as the size times the access count.
pub fn normal_priority_function(access_count: usize, size: usize) -> usize {
    match usize::checked_mul(size, access_count) {
        Some(v) => v,
        None => usize::MAX
    }
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
    usize::checked_div(usize::MAX, size).unwrap_or(0) // don't give any priority to completely empty files.
}

/// Favor small files with respect to the number of times file was accessed.
///
/// The smaller the file, the higher priority it will have.
/// Does take into account the number of accesses the file has.
pub fn small_files_access_priority_function(access_count: usize, size: usize) -> usize {
    match usize::checked_mul(usize::checked_div(usize::MAX, size).unwrap_or(0), access_count) {
        Some(v) => v,
        None => usize::MAX // If the multiplication overflows, then the file will have the maximum priority.
    }
}
