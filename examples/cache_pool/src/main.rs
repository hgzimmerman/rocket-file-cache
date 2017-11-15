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

    // I am currently a little fuzzy about Rocket's threading model
    // I would assume that an unwrap is safe here because there are an equal to or greater than number
    // of caches in the pool, than threads rocket is working with.
    // Assuming one thread per request, there will never be more locks taken out than there are
    // requests to be serviced, preventing a panic!() at the unwrap.
    cache_pool.try_get().unwrap().get(&path)
}


fn main() {

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

    // You should want the number of entries to correspond to the number of threads being used by Rocket
    // for servicing requests.
    // Any more, and you are wasting space.
    // Any less, and you can't just unwrap() the result of try_get(), as all caches may be locked at once.
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
}