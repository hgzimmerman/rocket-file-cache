# 0.12.0

### Features
* Automatically refresh files in the cache based on a specified number of accesses.

### Misc
* `Cache::refresh()` now returns a `CachedFile` instead of a `bool`.
* Responding with a `NamedInMemoryFile` will set the raw body of the response instead of streaming the file.
This should fix a bug regarding displaying estimated download times.

# 0.11.1
### Misc
* Removed `unwrap()`s from the Cache, making sure it will not `panic!`, even under very rare concurrent conditions.
* Improved documentation. 

# 0.11.0
### Misc
* `Cache::get()` now returns just a `CachedFile` instead of an `Option<CachedFile>`
    * `CachedFile` now handles the error case where the requested file cannot be found, supplanting the need for an `Option`.
    * A proper use of the cache now looks like: 
```rust
#[get("/<file..>")]
fn files(file: PathBuf, cache: State<Cache> ) -> CachedFile {
    CachedFile::open(Path::new("static/").join(file), cache.inner())
}
```

# 0.10.1
### Misc
* Improved documentation.

# 0.10.0
### Misc
* Moved `FileStats` struct into in_memory_file.rs.
* Removed capacity field from `CacheBuilder`.

# 0.9.0
### Misc
* `CacheBuilder::new()` now takes a `usize` indicating what the size limit (bytes) of the cache should be.
* `CacheBuilder::size_limit_bytes()` has been removed.
* Added ability to set concurrency setting of the cache's backing concurrent HashMaps.
* `NamedInMemoryFile` must be imported as `use rocket_file_cache::named_in_memory_file::NamedInMemoryFile` now.
* `PriorityFunction` type is no longer public.
* Made calculation of priority functions protected against overflowing,
 allowing the access count for files in the cache to be safely set to `usize::MAX` by calling `alter_access_count()`,
 effectively marking them as always in the cache.
 * `Cache::remove()` now returns a `bool`.
 

# 0.8.0
### Features
* The cache is now fully concurrent.
    * This means that the Cache no longer needs to be wrapped in a mutex.
    * Performance under heavy loads should be better.
    * Calling `cache.get()` will return a ResponderFile that may contain a lock bound to the file in the Cache.


### Misc
* `ResponderFile` is now named `CachedFile`.

### Regressions
* Use of this library with pools of caches as well as falling back to getting files from the FS if the cache is locked no longer work.

# 0.7.0
### Misc
* Public functions for `Cache` now take `P: AsRef<Path>` instead of `PathBuf` or `&PathBuf` now.

# 0.6.2
### Misc
* Fixed how incrementing access counts and updating stats for files in the cache works.
    * This should make performance for cache misses better.
    
    
# 0.6.1
### Misc
* Improved documentation.


# 0.6.0
### Features
* Added `Cache::alter_access_count()`, which allows the setting of the access count for a given file in the cache.
    * This allows for manual control of the priority of a given file.
* Added `Cache::alter_all_access_counts()`, which allows the setting of all access counts for every file monitored by the cache.

### Misc
* `Cache::get()` takes a `&PathBuf` instead of a `Pathbuf`.
* Moved `ResponderFile` into its own module.

# 0.5.0
### Misc
* Implemented `ResponderFile` as a replacement for `RespondableFile`.
    * `ResponderFile` is implemented as a normal enum, instead of the former tuple struct that wrapped an `Either<CachedFile,NamedFile>`.
 
# 0.4.1
### Misc
* Changed the priority functions to be functions instead of constants. 
* Changed `SizedFile` into `InMemoryFile`

### Bug Fixes
* Fixed a bug related to incrementing access counts and updating stats.

# 0.4.0
### Features
* Return a `RespondableFile`, which wraps either a `rocket::response::NamedFile`, or a `CachedFile`.
    * This allows the cache to return a NamedFile if it knows that the requested file will not make it into the cache.
    * This vastly improves performance on cache-misses because creating a CachedFile requires reading the whole file into memory,
    and then copying it when setting the response.
    Responding with a NamedFile sets the response's body by directly reading the file, which is faster.

# 0.3.0
### Features
* Added `CacheBuilder` that allows configuring the cache at instantiation.

### Misc
* Split project into multiple files.

# 0.2.2
### Misc
* Added another constructor that takes a priority function, as well as the maximum size.

# 0.2.1
### Misc
* Improved documentation

# 0.2.0
### Features
* Added priority functions, allowing consumers of the crate to set the algorithm used to determine which files make it into the cache.


# 0.1.0
* Initial publish