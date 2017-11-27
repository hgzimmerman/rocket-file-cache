#![feature(plugin, decl_macro)]
#![plugin(rocket_codegen)]

extern crate rocket;
extern crate rocket_file_cache;

use rocket_file_cache::{Cache, CachedFile};
use std::path::{Path, PathBuf};
use rocket::State;


#[get("/<file..>")]
fn files(file: PathBuf, cache: State<Cache> ) -> Option<CachedFile> {
    CachedFile::open(Path::new("www/").join(file), cache.inner())
}


fn main() {
    let cache: Cache = Cache::new(1024 * 1024 * 40); // 40 MB
    rocket::ignite()
        .manage(cache)
        .mount("/", routes![files])
        .launch();
}