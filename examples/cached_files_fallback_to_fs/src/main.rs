#![feature(plugin, decl_macro)]
#![plugin(rocket_codegen)]

extern crate rocket;
extern crate rocket_file_cache;

use rocket_file_cache::{Cache, ResponderFile};
use std::sync::Mutex;
use std::path::{Path, PathBuf};
use rocket::State;


#[get("/<file..>")]
fn files(file: PathBuf, cache: State<Mutex<Cache>> ) -> Option<ResponderFile> {
    let path: PathBuf = Path::new("www/").join(file).to_owned();

    // Try to lock the mutex in order to use the cache.
    match cache.try_lock() {
        Ok(mut cache) => cache.get(&path),
        Err(_) => {
            // Fall back to using the filesystem if another thread owns the lock.
            match NamedFile::open(path).ok() {
                Some(file) => Some(ResponderFile::from(file)),
                None => None
            }
        }
    }
}


fn main() {
    let cache: Mutex<Cache> = Mutex::new(Cache::new(1024 * 1024 * 40)); // 40 MB
    rocket::ignite()
        .manage(cache)
        .mount("/", routes![files])
        .launch();
}