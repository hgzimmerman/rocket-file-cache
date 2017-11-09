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
    let cache: Mutex<Cache> = Mutex::new(Cache::new(1024 * 1024 * 10)); // 10 megabytes
    rocket::ignite()
        .manage(cache)
        .mount("/", routes![files])
        .launch();
}

#[get("/<file..>")]
fn files(file: PathBuf, cache: State<Mutex<Cache>>) -> Option<CachedFile> {
    let pathbuf: PathBuf = Path::new("www/").join(file).to_owned();
    cache.lock().unwrap().get_or_cache(pathbuf)
}
```


# Should I use this?
Rocket File Cache keeps a set of frequently accessed files in memory so your webserver won't have to wait for your disk to read the files.
This should improve latency and throughput on systems that are bottlenecked on disk I/O.

Because the cache needs to be hidden behind a Mutex, only one thread can get access at a time.
This will have a negative performance impact in cases where the webserver is handling enough traffic to constantly cause lock contention.

# Performance
Running the bench tests currently in the repository on a computer with an Intel i7-6700K CPU @ 4.00GHz, with a Samsung NVME SSD 950 PRO 512 GB:
```
test cache::tests::cache_get_10mb       ... bench:   3,190,919 ns/iter (+/- 38,663)
test cache::tests::cache_get_1mb        ... bench:      30,922 ns/iter (+/- 448)
test cache::tests::cache_get_5mb        ... bench:   1,365,297 ns/iter (+/- 109,607)
test cache::tests::cache_miss_10mb      ... bench:   4,269,662 ns/iter (+/- 150,517)
test cache::tests::cache_miss_1mb       ... bench:     553,703 ns/iter (+/- 23,066)
test cache::tests::cache_miss_5mb       ... bench:   2,091,184 ns/iter (+/- 45,822)
test cache::tests::named_file_read_10mb ... bench:   3,782,855 ns/iter (+/- 209,363)
test cache::tests::named_file_read_1mb  ... bench:     538,282 ns/iter (+/- 17,881)
test cache::tests::named_file_read_5mb  ... bench:   1,846,324 ns/iter (+/- 404,341)

```

There are across the board improvements for accessing the cache instead of the filesystem, with more significant gains made for smaller files, even with hardware that should not necessitate the use of this library.
That said, because the cache is guarded by a Mutex, synchronous access is impeded, possibly slowing down the effective serving rate of the webserver.

This performance hit can be mitigated by using a pool of caches at the expense of increased memory use,
or by immediately falling back to getting files from the filesystem if a lock can't be gained.


I have seen significant speedups for servers that serve small files that are only sporadically accessed.
I cannot recommend the use of this library outside of that use case until further benchmarks are performed.

# Warning
This crate is still under development.


### Things that may change:
* Default priority function. Because performance seems to be most improved for smaller files, the default priority function
may change to favor smaller files in the future.
* Currently the priority scores for all files are calculated every time a file tries to be added to the cache.
 This may slow down cache misses / insertions for caches containing many files.
 Instead priority scores in the future may be stored in the cache, and updated when the access count is incremented.
 This will cause the checks when trying to add files to the cache to happen more quickly, but will also slightly slow down
 cache hits.

# Documentation
Documentation can be found here: https://docs.rs/crate/rocket-file-cache
