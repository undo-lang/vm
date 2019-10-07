use std::env;
use std::fs::File;
use std::io::Read;

extern crate lib;

fn main() {
  let mut content = String::new();
  let result;

  if let Some(path) = env::args().nth(1) {
    println!("Loading file {}", path);

    match File::open(&path) {
      Ok(mut file) =>
        result = file.read_to_string(&mut content),
      Err(err) =>
        panic!("Can't read the file: {}", err),
    }
  } else {
    result = std::io::stdin().read_to_string(&mut content)
  }

  match result {
    Ok(_) =>
      match serde_json::from_str(&content) {
        Ok(module) =>
          lib::vm::run(module),
        Err(err) =>
          println!("Couldn't parse json: {}", err),
      }
    Err(err) =>
      println!("An error occured trying to read the file: {}", err),
  }
}
