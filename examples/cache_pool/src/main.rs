#![feature(plugin, decl_macro)]
#![plugin(rocket_codegen)]

extern crate rocket;
extern crate rocket_file_cache;
extern crate rand;

use rocket_file_cache::{Cache, ResponderFile};
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::path::{Path, PathBuf};
use rocket::State;
use std::sync::Arc;
use rand::{thread_rng, Rng};

#[get("/<file..>")]
fn files(file: PathBuf, cache_pool: State<Pool> ) -> Option<ResponderFile> {
    let path: PathBuf = Path::new("www/").join(file).to_owned();

    cache_pool.try_get().unwrap().get(&path)
}


fn main() {

    // 50MB * 4 means up to 200 MB can be allocated for the cache, but will only store 50 MB worth of files
    let pool : Pool =  Pool::new(4, || Cache::new(1024 * 1024 * 50));
    rocket::ignite()
        .manage(pool)
        .mount("/", routes![files])
        .launch();
}


/// A pool that holds many caches.
/// The caches held by the pool are NOT guaranteed to have the same files in them.
/// Assuming random access of caches from the pool, and a large enough sample, the content's of the
/// owned caches should trend towards being the same.
#[derive(Clone, Debug)]
struct Pool {
    caches: Vec<Arc<Mutex<Cache>>>
}

impl Pool {
    fn new(number_of_entries: usize, cache_creation_function: fn() -> Cache) -> Pool {
        let mut caches: Vec<Arc<Mutex<Cache>>> = vec!();

        for _ in 0..number_of_entries {
            caches.push(Arc::new(Mutex::new(cache_creation_function())))
        }
        Pool {
            caches
        }
    }


    fn try_get<'a>(&'a self) -> Option<MutexGuard<'a, Cache>> {

        // Randomize the range that can be accessed
        let mut range: Vec<usize> = (0..self.caches.len()).collect();
        thread_rng().shuffle(range.as_mut_slice());

        for i in range.into_iter() {
            if let Some(c) = self.caches[i].try_lock().ok() {
                println!("Locked cache {}", i);
                return Some(c) // Found a cache that wasn't locked
            }
        }
        None // All caches are occupied
    }


    /// Alter every element in the pool by locking them one at a time.
    ///
    /// # Warning
    /// If a lock is held elsewhere, and the provided function would cause that element to remain locked
    /// this has the possibility to deadlock.
    fn alter_all<'a>(&'a self, function: fn(MutexGuard<'a, Cache>) ) {
        for e in self.caches.iter() {
            // all entries in the pooled try to lock, one at a time, so that the provided function
            // can operate on the pool's contents.
            function(e.lock().unwrap())
        }
    }
}