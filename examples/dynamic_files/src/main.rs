#![feature(plugin, decl_macro)]
#![plugin(rocket_codegen)]

extern crate rocket;
extern crate rocket_file_cache;

use rocket_file_cache::{Cache, CachedFile};
use std::path::{Path, PathBuf};
use std::fs;
use std::io;
use rocket::State;
use rocket::Data;


#[get("/<file..>")]
fn files(file: PathBuf, cache: State<Cache> ) -> Option<CachedFile> {
    let path: PathBuf = Path::new("www/").join(file).to_owned();
    cache.inner().get(&path) // Getting the file will add it to the cache if there is room.
}

#[post("/<file..>", data = "<data>")]
fn upload(file: PathBuf, data: Data) -> io::Result<String> {
    let path: PathBuf = Path::new("www/").join(file).to_owned();
    if path.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "File at path already exists."))
    }
    data.stream_to_file(path).map(|n| n.to_string())
}

#[put("/<file..>", data = "<data>")]
fn update(file: PathBuf, data: Data, cache: State<Cache>) -> io::Result<String> {
    let path: PathBuf = Path::new("www/").join(file).to_owned();
    let result = data.stream_to_file(path.clone()).map(|n| n.to_string());

    cache.refresh(&path); // Make sure the file in the cache is updated to reflect the FS.
    result
}

#[delete("/<file..>")]
fn remove(file: PathBuf, cache: State<Cache>) {
    let path: PathBuf = Path::new("www/").join(file).to_owned();
    fs::remove_file(&path); // Remove the file from the FS.
    {
        cache.remove(&path); // Remove the file from the cache.
    }
    {
        // Reset the count to 0 so if a file with the same name is added in the future,
        // it won't immediately have the same priority as the file that was deleted here.
        cache.alter_access_count(&path, |_x| 0);
    }
}


fn main() {
    let cache: Cache = Cache::new(1024 * 1024 * 20); // 20 MB
    rocket::ignite()
        .manage(cache)
        .mount("/", routes![files, remove, upload, update])
        .launch();
}