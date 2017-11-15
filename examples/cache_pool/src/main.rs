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
fn files(file: PathBuf, cache_pool: State<Pool<Cache>> ) -> Option<ResponderFile> {
    let path: PathBuf = Path::new("www/").join(file).to_owned();

    // I am currently a little fuzzy about Rocket's threading model
    // I would assume that an unwrap is safe here because there are an equal to or greater than number
    // of caches in the pool, than threads rocket is working with.
    // Assuming one thread per request, there will never be more locks taken out than there are
    // requests to be serviced, preventing a panic!() at the unwrap.
    cache_pool.try_get().unwrap().get(&path)
}


fn main() {

    // 50MB * 4 means up to 200 MB can be allocated for the cache, but will only store 50 MB worth of files
    let pool : Pool<Cache> =  Pool::new(4, || Cache::new(1024 * 1024 * 50));
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
struct Pool<T> {
    elements: Vec<Arc<Mutex<T>>>
}

impl<T> Pool<T> {

    // You should want the number of entries to correspond to the number of threads being used by Rocket
    // for servicing requests.
    // Any more, and you are wasting space.
    // Any less, and you can't safely unwrap() the result of try_get(), as all caches may be locked at once.
    fn new(number_of_entries: usize, element_creation_function: fn() -> T) -> Pool<T> {
        let mut elements: Vec<Arc<Mutex<T>>> = vec!();

        for _ in 0..number_of_entries {
            elements.push(Arc::new(Mutex::new(element_creation_function())))
        }
        Pool {
            elements: elements
        }
    }


    /// Try to get a random element from the pool.
    /// If all elements are locked, this will return `None`.
    ///
    /// This will not deadlock.
    fn try_get<'a>(&'a self) -> Option<MutexGuard<'a, T>> {

        // Randomize the range that can be accessed
        let mut range: Vec<usize> = (0..self.elements.len()).collect();
        thread_rng().shuffle(range.as_mut_slice());

        for i in range.into_iter() {
            if let Some(c) = self.elements[i].try_lock().ok() {
                println!("Locked element at index: {}", i);
                return Some(c) // Found a cache that wasn't locked
            }
        }
        None // All caches are occupied
    }

    /// Blocks before returning a random element from the pool.
    ///
    /// This has the possibility to deadlock if all locks are owned.
    fn get<'a>(&'a self) -> MutexGuard<'a, T> {
        // Randomize the range that can be accessed
        let mut range: Vec<usize> = (0..self.elements.len()).collect();
        thread_rng().shuffle(range.as_mut_slice());

        let mut index: usize = 0;
        loop {
            match self.elements[index].try_lock().ok() {
                Some(element) => return element,
                None => index = (index + 1) % self.elements.len()
            }
        }
    }


    /// Alter every element in the pool by locking them one at a time.
    ///
    /// # Warning
    /// If a lock for one of the pooled elements is held elsewhere, and the provided function
    /// would cause that element to remain locked, this has the possibility to deadlock.
    fn alter_all<'a>(&'a self, function: fn(MutexGuard<'a, T>) ) {
        for e in self.elements.iter() {
            // all entries in the pooled try to lock, one at a time, so that the provided function
            // can operate on the pool's contents.
            function(e.lock().unwrap())
        }
    }
}