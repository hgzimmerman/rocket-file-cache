[![Current Crates.io Version](https://img.shields.io/crates/v/rocket-file-cache.svg)](https://crates.io/crates/rocket-file-cache)

# Rocket File Cache
A concurrent, in-memory file cache for the Rocket web framework.

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
fn files(file: PathBuf, cache: State<Cache> ) -> Option<CachedFile> {
    CachedFile::open(Path::new("static/").join(file), cache.inner())
}


fn main() {
    let cache: Cache = Cache::new(1024 * 1024 * 40); // 40 MB
    rocket::ignite()
        .manage(cache)
        .mount("/", routes![files])
        .launch();
}
```


# Use case 
Rocket File Cache keeps a set of frequently accessed files in memory so your webserver won't have to wait for your disk to read the files.
This should improve latency and throughput on systems that are bottlenecked on disk I/O.

In environments where the files being served can all fit comfortably into the cache, and the files themselves are small, significant speedups for serving the files have been observed.

If you are serving a known size of static files (index.html, js bundle, a couple of assets),
you should try to set the maximum size of the cache to let them all fit,
especially if all of these are served every time someone visits your website.

If you serve static files with a larger aggregate size than what would fit into memory, 
but you have some content that is visited more often than others, you should specify enough space for the cache
so that the most popular content will fit.
If your popular content changes over time, and you want the cache to reflect what is currently most popular,
it is possible to use the `alter_access_count()` method to reduce the access count of all items currently in the cache,
making it easier for newer content to find its way into the cache.


If you serve user created files, the same logic regarding file popularity applies,
only that you may want to spawn a thread every 10000 or so requests that will use `alter_access_count()` 
to reduce the access counts of the items in the cache.

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
Minimum and maximum file sizes can be set to keep files in the cache within size bounds.

For queries that will retrieve an entry from the cache, there is no time penalty for each additional file in the cache.
The more items in the cache, the larger the time penalty for a cache miss.



### Requirements
* Rocket >= 0.3.3
* Nightly Rust
  * Known to work on (2017-10-22). 
  This crate doesn't use any special features of nightly, but is dependent on Rocket, which currently requires nightly.


# Notes
If you have any feature requests or notice any bugs, please open an Issue.

I am rapidly developing this crate.
I am changing the public interface on a regular basis.
You should expect a few breaking changes before this reaches a 1.0.0 release.
You can keep up to date with these changes with the [changelog](CHANGELOG.md)

# Alternatives 
* [Nginx](http://nginx.org/)
* Write your own.
Most of the work here focuses on when to replace items in the cache.
If you know that you will never grow or shrink your cache of files, all you need is a 
`Mutex<HashMap<PathBuf, Vec<u8>>>`, an `impl Responder<'static> for Vec<u8> {...}`, and some glue logic
to hold your files in memory and serve them as responses.
