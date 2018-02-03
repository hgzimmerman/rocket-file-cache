#![feature(plugin, decl_macro)]
#![plugin(rocket_codegen)]

extern crate rocket;
extern crate rocket_file_cache;

#[cfg(test)]
mod tests;

use rocket_file_cache::{Cache, CacheBuilder, CachedFile};
use std::path::{Path, PathBuf};
use rocket::State;
use rocket::Rocket;


#[get("/<file..>")]
fn files(file: PathBuf, cache: State<Cache> ) -> CachedFile {
    CachedFile::open(Path::new("www/").join(file), cache.inner())
}


fn main() {
    rocket().launch();
}

fn rocket() -> Rocket {
    let cache: Cache = CacheBuilder::new()
        .size_limit(1024 * 1024 * 40) // 40Mb
        .build()
        .unwrap();
    rocket::ignite()
        .manage(cache)
        .mount("/", routes![files])
}