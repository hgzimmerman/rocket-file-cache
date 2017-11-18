#![feature(plugin, decl_macro)]
#![plugin(rocket_codegen)]

extern crate rocket;
extern crate rocket_file_cache;
extern crate random_pool;

use rocket_file_cache::{Cache, CachedFile};
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::path::{Path, PathBuf};
use rocket::State;
use random_pool::RandomPool;
use std::sync::Arc;


#[get("/<file..>")]
fn files<'a>(file: PathBuf, cache_pool: State<'a, RandomPool<Cache>> ) -> Option<CachedFile<'a>> {
    let path: PathBuf = Path::new("www/").join(file).to_owned();

    // I am currently a little fuzzy about Rocket's threading model
    // I would assume that an unwrap is safe here because there are an equal to or greater than number
    // of caches in the pool, than threads rocket is working with.
    // Assuming one thread per request, there will never be more locks taken out than there are
    // requests to be serviced, preventing a panic!() at the unwrap.
    CachedFile::open(path, &cache_pool.inner().try_get().unwrap())
    // TODO this will not compile because the lock does not last long enough.
}


fn main() {

    // 50MB * 4 means up to 200 MB can be allocated for the cache, but will only store 50 MB worth of files
    let pool: RandomPool<Cache> =  RandomPool::new(4, || Cache::new(1024 * 1024 * 50));
    rocket::ignite()
        .manage(pool)
        .mount("/", routes![files])
        .launch();
}
