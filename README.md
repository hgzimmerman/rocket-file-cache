# Rocket File Cache
An in-memory file cache for the Rocket web framework.

Rocket File Cache can be used as a drop in replacement for Rocket's NamedFile when serving files.

This code from the [static_files](https://github.com/SergioBenitez/Rocket/blob/master/examples/static_files/src/main.rs) example from Rocket:
```rust
#[get("/<file..>")]
fn files(file: PathBuf) -> Option<NamedFile> {
    NamedFile::open(Path::new("static/").join(file)).ok()
}

fn main() {
    rocket::ignite().mount("/", routes![files]).launch();
}
```
Can be sped up by getting files via a cache instead:
```rust
#[get("/<file..>")]
fn files(file: PathBuf, cache: State<Mutex<Cache>>) -> Option<ResponderFile> {
    let pathbuf: PathBuf = Path::new("static/").join(file).to_owned();
    cache.lock().unwrap().get(&pathbuf)
}

fn main() {
    let cache: Mutex<Cache> = Mutex::new(Cache::new(1024 * 1024 * 40)); // 40 MB
    rocket::ignite()
        .manage(cache)
        .mount("/", routes![files])
        .launch();
}
```


# Should I use this?
Rocket File Cache keeps a set of frequently accessed files in memory so your webserver won't have to wait for your disk to read the files.
This should improve latency and throughput on systems that are bottlenecked on disk I/O.

In environments where the files in question can all fit comfortably into the cache, and the files themselves are small, significant speedups for serving the files have been observed.

Rocket File Cache has not been tested yet in an environment where large files ( > 10MB ) are fighting to be kept in the cache, but small improvements in performance should be expected there.

### Performance

The bench tests try to get the file from whatever source, either cache or filesystem, and read it once into memory.
The misses measure the time it takes for the cache to realize that the file is not stored, and to read the file from disk.
Running the bench tests on an AWS EC2 t2 micro instance (82 MB/s HDD) returned these results:
```
test cache::tests::cache_get_10mb                       ... bench:   1,444,068 ns/iter (+/- 251,467)
test cache::tests::cache_get_1mb                        ... bench:      79,397 ns/iter (+/- 4,613)
test cache::tests::cache_get_1mb_from_1000_entry_cache  ... bench:      79,038 ns/iter (+/- 1,751)
test cache::tests::cache_get_5mb                        ... bench:     724,262 ns/iter (+/- 7,751)
test cache::tests::cache_miss_10mb                      ... bench:   3,184,473 ns/iter (+/- 299,657)
test cache::tests::cache_miss_1mb                       ... bench:     806,821 ns/iter (+/- 19,731)
test cache::tests::cache_miss_1mb_from_1000_entry_cache ... bench:   1,379,925 ns/iter (+/- 25,118)
test cache::tests::cache_miss_5mb                       ... bench:   1,542,059 ns/iter (+/- 27,063)
test cache::tests::cache_miss_5mb_from_1000_entry_cache ... bench:   2,090,871 ns/iter (+/- 37,040)
test cache::tests::in_memory_file_read_10mb             ... bench:   7,222,402 ns/iter (+/- 596,325)
test cache::tests::named_file_read_10mb                 ... bench:   4,908,544 ns/iter (+/- 581,408)
test cache::tests::named_file_read_1mb                  ... bench:     893,447 ns/iter (+/- 18,354)
test cache::tests::named_file_read_5mb                  ... bench:   1,605,741 ns/iter (+/- 41,418)
```

It can be seen that on a server with slow disk reads, small file access times are vastly improved versus the disk.
Larger files also seem to benefit, although to a lesser degree.
A maximum file size can be set to prevent files above a specific size from being added.

Because the cache needs to be guarded by a Mutex, only one thread can get access at a time.
This will have a negative performance impact in cases where the webserver is handling enough traffic to constantly cause lock contention.

This performance hit can be mitigated by using a pool of caches at the expense of increased memory use,
or by immediately falling back to getting files from the filesystem if a lock can't be gained.

For queries that will retrieve an entry from the cache, there is no time penalty for each additional file in the cache.
The more items in the cache, the larger the time penalty for a cache miss.

### Requirements
* Nightly Rust (2017-10-22)
* Rocket >= 0.3.3

# Notes
If you have any feature requests or notice any bugs, please open an Issue.


# Documentation
Documentation can be found here: https://docs.rs/crate/rocket-file-cache
