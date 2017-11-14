# Rocket File Cache
An in-memory file cache for the Rocket web framework.

Rocket File Cache can be used as a drop in replacement for Rocket's NamedFile when serving files.

This:
```rust
#[get("/<file..>")]
fn files(file: PathBuf) -> Option<NamedFile> {
    NamedFile::open(Path::new("static/").join(file)).ok()
}
```
Can be replaced with:
```rust
fn main() {
    let cache: Mutex<Cache> = Mutex::new(Cache::new(1024 * 1024 * 40)); // 40 megabytes
    rocket::ignite()
        .manage(cache)
        .mount("/", routes![files])
        .launch();
}

#[get("/<file..>")]
fn files(file: PathBuf, cache: State<Mutex<Cache>>) -> Option<ResponderFile> {
    let pathbuf: PathBuf = Path::new("www/").join(file).to_owned();
    cache.lock().unwrap().get(&pathbuf)
}
```


# Should I use this?
Rocket File Cache keeps a set of frequently accessed files in memory so your webserver won't have to wait for your disk to read the files.
This should improve latency and throughput on systems that are bottlenecked on disk I/O.

In environments where the files in question can all fit comfortably into the cache, and the files themselves are small, it has been observed that webpage load times have seen > 3x improvements.

Rocket File Cache has not been tested yet in an environment where large files are fighting to be kept in the cache, but small improvements in performance should be expected there.

## Performance

The bench tests try to get the file from whatever source, and read it once into memory.
The misses measure the time it takes for the cache to realize that the file is not stored, and to read the file from disk.
Running the unscientific bench tests on an AWS EC2 t2 micro instance (82 MB/s HDD) returned these results:
```
test cache::tests::cache_get_10mb                       ... bench:   3,886,350 ns/iter (+/- 328,979)
test cache::tests::cache_get_1mb                        ... bench:      76,307 ns/iter (+/- 4,262)
test cache::tests::cache_get_1mb_from_1000_entry_cache  ... bench:      74,306 ns/iter (+/- 1,390)
test cache::tests::cache_get_5mb                        ... bench:   1,715,183 ns/iter (+/- 288,559)
test cache::tests::cache_miss_10mb                      ... bench:   5,234,225 ns/iter (+/- 526,213)
test cache::tests::cache_miss_1mb                       ... bench:   1,041,573 ns/iter (+/- 22,910)
test cache::tests::cache_miss_1mb_from_1000_entry_cache ... bench:      79,631 ns/iter (+/- 2,536)
test cache::tests::cache_miss_5mb                       ... bench:   3,087,674 ns/iter (+/- 53,189)
test cache::tests::cache_miss_5mb_from_1000_entry_cache ... bench:   1,738,275 ns/iter (+/- 20,879)
test cache::tests::named_file_read_10mb                 ... bench:   4,905,043 ns/iter (+/- 844,470)
test cache::tests::named_file_read_1mb                  ... bench:   1,037,299 ns/iter (+/- 15,554)
test cache::tests::named_file_read_5mb                  ... bench:   2,358,750 ns/iter (+/- 65,191)
test cache::tests::sized_file_read_10mb                 ... bench:  14,617,872 ns/iter (+/- 1,524,981)
```

It can be seen that on a server with slow disk reads, small file access times are vastly improved versus the disk.
Larger files also seem to benefit, although to a lesser degree.
A maximum file size can be set to prevent files above a specific size from being added.

Because the cache needs to be hidden behind a Mutex, only one thread can get access at a time.
This will have a negative performance impact in cases where the webserver is handling enough traffic to constantly cause lock contention.

This performance hit can be mitigated by using a pool of caches at the expense of increased memory use,
or by immediately falling back to getting files from the filesystem if a lock can't be gained.

For queries that will retrieve an entry from the cache, there is no time penalty for each additional file in the cache.
The more items in the cache, the larger the time penalty for a cache miss.

# Warning
This crate is still under development.

Public functions and structures are still subject to change, but are mostly stabilized at this point.

### Things that may change:
* Default priority function. Because performance seems to be most improved for smaller files, the default priority function
may change to favor smaller files in the future.


# Documentation
Documentation can be found here: https://docs.rs/crate/rocket-file-cache
