use std::fs::File;
use std::io::Read;

use rocket::local::Client;
use rocket::http::Status;

use super::rocket;

fn test_query_file<T> (path: &str, file: T, status: Status)
    where T: Into<Option<&'static str>>
{
    let client = Client::new(rocket()).unwrap();
    let mut response = client.get(path).dispatch();
    assert_eq!(response.status(), status);

    let body_data = response.body().and_then(|body| body.into_bytes());
    if let Some(filename) = file.into() {
        let expected_data = read_file_content(filename);
        assert!(body_data.map_or(false, |s| s == expected_data));
    }
}

fn read_file_content(path: &str) -> Vec<u8> {
    let mut fp = File::open(&path).expect(&format!("Can not open {}", path));
    let mut file_content = vec![];

    fp.read_to_end(&mut file_content).expect(&format!("Reading {} failed.", path));
    file_content
}

#[test]
fn test_get_file() {
    test_query_file("/test.txt", "www/test.txt", Status::Ok);
    test_query_file("/test.txt?v=1", "www/test.txt", Status::Ok);
    test_query_file("/test.txt?this=should&be=ignored", "www/test.txt", Status::Ok);
}