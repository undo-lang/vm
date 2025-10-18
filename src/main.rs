use std::{env, fs::File, io::Read};
use lib::{bc, vm};

extern crate lib;

fn load_module(path: String) -> Result<bc::Module, String> {
    let mut content = String::new();
    if path == "-" {
        std::io::stdin()
            .read_to_string(&mut content)
            .expect("Cannot read stdin");
    } else {
        let mut file = File::open(&path).map_err(|err| err.to_string())?;
        file.read_to_string(&mut content)
            .expect(format!("Cannot read the file {path}").as_str());
    }
    serde_json::from_str(&content).map_err(|err| err.to_string())
}

fn main() {
    let mut main: Vec<String> = Vec::new();
    let mut modules: Vec<bc::Module> = Vec::new();

    // XXX this means `./undo-frontend` just errors, instead of behaving like `./undo-frontend -`
    for arg in env::args().skip(1) {
        eprintln!("Loading {}", arg);

        let module = load_module(arg.clone()).expect(format!("Cannot open module {arg}").as_str());
        let module_name = module.name.clone();
        if main.is_empty() {
            main = module_name.clone();
        }
        modules.push(module);
    }

    vm::run(main, modules);
}
