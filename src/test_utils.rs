#![allow(unused)]

use rusqlite::{Connection, Result};
use std::path::Path;

pub struct TestDatabase {
    connection: Connection,
    file_path: std::path::PathBuf,
}

impl TestDatabase {
    fn new(file_path: &Path) -> Result<Self> {
        let connection = Connection::open(file_path)?;
        Ok(TestDatabase {
            connection,
            file_path: file_path.to_path_buf(),
        })
    }
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        if std::fs::metadata(&self.file_path).is_ok() {
            std::fs::remove_file(&self.file_path).unwrap();
        }
    }
}

const CREATE_TABLE: &str = r#"
        CREATE TABLE users(
            id INTEGER PRIMARY KEY,
            name TEXT
        )
    "#;
const CREATE_INDEX: &str = "CREATE INDEX users_idx ON users(name)";
const INSERT_USERS: &str = "INSERT INTO users(name) VALUES(?)";
const NAMES: [&str; 3] = ["Alice", "Bob", "Charlie"];

pub fn init_db(file_path: &Path) -> TestDatabase {
    let db = TestDatabase::new(file_path).unwrap();

    db.connection.execute(CREATE_TABLE, []).unwrap();
    db.connection.execute(CREATE_INDEX, []).unwrap();
    NAMES.iter().for_each(|name| {
        db.connection.execute(INSERT_USERS, [name]).unwrap();
    });
    db
}
