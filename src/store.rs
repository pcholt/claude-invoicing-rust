use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    dirs::home_dir()
        .expect("Cannot determine home directory")
        .join(".local")
        .join("share")
        .join("inv")
}

pub fn read_json<T: DeserializeOwned + Default>(filename: &str) -> T {
    let dir = data_dir();
    fs::create_dir_all(&dir).expect("Cannot create data directory");
    let path = dir.join(filename);
    if !path.exists() {
        return T::default();
    }
    let content = fs::read_to_string(&path).expect("Cannot read data file");
    serde_json::from_str(&content).unwrap_or_default()
}

pub fn write_json<T: Serialize + ?Sized>(filename: &str, data: &T) {
    let dir = data_dir();
    fs::create_dir_all(&dir).expect("Cannot create data directory");
    let path = dir.join(filename);
    let content = serde_json::to_string_pretty(data).expect("Cannot serialize data");
    fs::write(path, content).expect("Cannot write data file");
}
