/// Custom type of function that is used to determine how to add files to the cache.
/// The first term will be assigned the access count of the file in question, while the second term will be assigned the size (in bytes) of the file in question.
/// The result will represent the priority of the file to remain in or be added to the cache.
/// The files with the largest priorities will be kept in the cache.
///
/// A closure that matches this type signature can be specified at cache instantiation to define how it will keep items in the cache.
pub type PriorityFunction = fn(usize, usize) -> usize;


/// The default priority function used for determining if a file should be in the cache
/// This function takes the square root of the size of the file times the number of times it has been accessed.
fn balanced_priority(access_count: usize, size: usize) -> usize {
    ((size as f64).sqrt() as usize) * access_count
}
pub const DEFAULT_PRIORITY_FUNCTION: PriorityFunction = balanced_priority;

/// This priority function will value files in the cache based solely on the number of times the file is accessed.
fn access_priority(access_count: usize, _: usize) -> usize {
    access_count
}
pub const ACCESS_PRIORITY_FUNCTION: PriorityFunction = access_priority;