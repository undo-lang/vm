use std::env;
use std::fs::File;
use std::io::Read;
use std::collections::HashMap;
use lib::vm::Module;

extern crate lib;

fn main() {
  let mut content = String::new();
  let result;

  if let Some(path) = env::args().nth(1) {
    eprintln!("Loading file {}", path);

    match File::open(&path) {
      Ok(mut file) =>
        result = file.read_to_string(&mut content),
      Err(err) =>
        panic!("Can't read the file: {}", err),
    }
  } else {
    result = std::io::stdin().read_to_string(&mut content)
  }

  // TODO collect+load deps
  // TODO probably have `modules` instead of deps+pass name of module to run
  let deps: HashMap<Vec<String>, Module> = HashMap::new();
  match result {
    Ok(_) =>
      match serde_json::from_str(&content) {
        Ok(module) =>
          lib::vm::run(module, deps),
        Err(err) =>
          eprintln!("Couldn't parse json: {}", err),
      }
    Err(err) =>
      eprintln!("An error occurred trying to read the file: {}", err),
  }
}
