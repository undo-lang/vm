use std::collections::{BTreeMap as Map, HashMap, HashSet};
use std::collections::VecDeque;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde()]
struct ModuleName {
  module: Vec<String>
}

fn is_prelude(module_name: &ModuleName) -> bool {
  module_name.module.len() == 1 && module_name.module[0] == "Prelude"
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "tag", content = "contents")]
enum Instruction {
  PushInt(i64),
  PushString(usize),
  LoadLocal(usize),
  StoreLocal(usize),
  // LoadName(Vec<String>, String),
  LoadName(ModuleName, String),
  LoadGlobal(String),
  Unless(usize),
  Jump(usize),
  Call(usize),
}

#[derive(Serialize, Deserialize)]
pub struct Module {
  name: Vec<String>,
  strings: Vec<String>,
  functions: Map<String, Vec<Instruction>>,
  dependencies: Vec<Vec<String>>,
}

struct Frame<'a> {
  module: &'a Module,
  fun: String,
  ip: usize,
  base: usize,
  locals: Vec<Ptr> // XXX we'll want to serialize this when we store closures
                   //     this will prevent captures from being gc'd
}

fn make_frame(module: &Module, name: String, base: usize) -> Frame {
  Frame {
    module,
    fun: name,
    ip: 0,
    base,
    locals: vec!()
  }
}

fn cur_fn(module: &Module, fn_name: String) -> &Vec<Instruction> {
  module.functions.get(&fn_name).expect("No such fn")
}

enum Value {
  IntVal(i64),
  StrVal(String),
  ModuleFnRef(Vec<String>, String),
  ThwartPtr(usize),
}

impl Value {
  fn to_string(&self) -> String {
    match self {
      Value::IntVal(i) => i.to_string(),
      Value::StrVal(s) => s.to_string(),
      Value::ModuleFnRef(_, f) => f.to_string(),
      Value::ThwartPtr(_) => "Thwart ptr".to_string()
    }
  }
}

impl Clone for Value {
  fn clone(&self) -> Value {
    match self {
      Value::IntVal(i) => Value::IntVal(*i),
      Value::StrVal(s) => Value::StrVal(s.to_string()),
      Value::ModuleFnRef(ns, f) => Value::ModuleFnRef(ns.iter().map(|s| s.to_string()).collect(), f.to_string()),
      Value::ThwartPtr(i) => Value::ThwartPtr(*i)
    }
  }
}

// TODO we shouldn't have a single value type
struct GC(Vec<Value>); // TODO 2nd arena
#[derive(Clone, Copy)]
struct Ptr(usize); //, usize);

fn compact(mut gc: GC, arena: Vec<Value>, frames: VecDeque<Frame>, mut stack: Vec<Ptr>) -> Vec<Value> {
  let mut new_arena: Vec<Value> = vec!();
  for (i, mut ptr) in stack.iter_mut().enumerate() {
    match gc.raw_at(i) {
      Value::ThwartPtr(i) => ptr.0 = *i, // Rewrite ptr
      v => {
        // TODO potentially traverse into `v`
        new_arena.push(v.clone());
        ptr.0 = new_arena.len() - 1;
        gc.set(i, Value::ThwartPtr(ptr.0));
      }
    }
  }
  for frame in frames {
    for local in frame.locals {
    }
  }
  new_arena
}

impl GC {
  fn at(&self, i: Ptr) -> &Value {
   self.raw_at(i.0)
  }

  fn raw_at(&self, i: usize) -> &Value {
    self.0.get(i).unwrap()
  }

  fn alloc(&mut self, v: Value) -> Ptr {
    self.0.push(v);
    Ptr(self.0.len() - 1)
  }

  fn set(&mut self, i: usize, v: Value) {
    // TODO assert i <= self.0.len
    self.0[i] = v;
  }

  fn new() -> Self {
    GC { 0: Vec::new() }
  }
}

fn run_main(module: Module, deps: HashMap<Vec<String>, Module>) {
  let mut gc = GC::new();
  let mut stack: Vec<Ptr> = Vec::new();
  let mut frames: VecDeque<Frame> = VecDeque::new();
  frames.push_back(make_frame(&module, "MAIN".to_string(), 0));

  while frames.len() > 0 {
    let mut cur_frame = frames.back_mut().unwrap();
    let fun = cur_fn(&module, cur_frame.fun.to_string());
    eprintln!("ip: {}", cur_frame.ip);
    eprintln!("got: {:?}", fun.get(cur_frame.ip));

    match fun.get(cur_frame.ip) {
      Some(Instruction::PushInt(n)) => {
        stack.push(gc.alloc(Value::IntVal(*n)));
        cur_frame.ip += 1;
      },

      Some(Instruction::PushString(n)) => {
        let string = cur_frame.module.strings.get(*n).expect("No such string");
        stack.push(gc.alloc(Value::StrVal(string.to_string())));
        cur_frame.ip += 1;
      },

      Some(Instruction::LoadLocal(idx)) => {
        let ptr = cur_frame.locals.get(*idx).expect("Trying to access uninitialized local");
        stack.push(*ptr);
        cur_frame.ip += 1;
      },

      Some(Instruction::StoreLocal(idx)) => {
        let ptr = stack.pop().expect("Stack is empty, cannot store");
        if cur_frame.locals.len() > *idx {
          cur_frame.locals[*idx] = ptr;
        } else if cur_frame.locals.len() == *idx {
          cur_frame.locals.push(ptr);
        } else {
          panic!("Out-of-order local initialization!");
        }
        cur_frame.ip += 1;
      }

      Some(Instruction::LoadName(namespace, name)) => {
        if is_prelude(namespace) || deps.contains_key(&namespace.module) {
          stack.push(gc.alloc(Value::ModuleFnRef(namespace.module.clone(), name.clone())));
        } else {
          eprintln!("Wrong module: {:?}", namespace);
          panic!("Trying to access to an un-loaded/unprovided module");
        }
        cur_frame.ip += 1;
      }

      Some(Instruction::LoadGlobal(name)) => {
        // TODO make sure the function exists
        stack.push(gc.alloc(Value::ModuleFnRef(cur_frame.module.name.clone(), name.clone())));
        cur_frame.ip += 1;
      },

      Some(Instruction::Jump(offset)) => {
        cur_frame.ip = *offset;
      },

      Some(Instruction::Unless(offset)) => {
        let ptr = stack.pop().expect("Nothing left on stack");
        let value = gc.at(ptr);
        match value {
          Value::IntVal(n) =>
              if *n == 0i64 {
                cur_frame.ip = *offset
              } else {
                cur_frame.ip += 1
              }
          _ =>
            cur_frame.ip += 1
        }
      },

      Some(Instruction::Call(arg_num)) => {
        // TODO need to think of a story for local functions and returning closures
        // one of the first thing we need is probably at semantic analysis stage. extract them to
        // be fake functions, and have an instruction to curry them, i.e.:
        // ModuleFnRefWithLocals([String], String, Locals: vec<Ptr>)
        let ptr = stack.pop().expect("Nothing left on stack to call");
        let value = gc.at(ptr);
        match value {
          Value::ModuleFnRef(ns, name) if ns.len() == 1 && ns[0] == "Prelude" => {
            match name.as_str() {
              "print" =>
                //for _ in (1..=*arg_num).rev()
                for _ in 1..=*arg_num {
                  println!("{}", gc.at(stack.pop().unwrap()).to_string());
                }
              _ => panic!("No such prelude fn: {name}", name = name)
            }
            cur_frame.ip += 1;
          },

          Value::ModuleFnRef(ns, name) => {
            // TODO load from ^^ ns, not from our current module...
            // NOTE: increment IP here, since adding a frame will invalid our borrow
            cur_frame.ip += 1;
            let mut new_frame = if ns == &module.name {
                make_frame(&module, name.to_string(), stack.len())
              } else {
              make_frame(deps.get(ns).unwrap(), name.to_string(), stack.len())
            };
            // Reverse arguments because we push(pop())
            for _ in (1..=*arg_num).rev() {
              new_frame.locals.push(stack.pop().unwrap());
            }
            frames.push_back(new_frame);
          },
          _ => {
            panic!("Can't call!");
          }
        }
      },

      None => {
        if stack.len() < cur_frame.base {
          panic!("Consumed too much stack space!");
        }
        while stack.len() > cur_frame.base {
          stack.pop();
        }
        frames.pop_back().expect("No current frame?!");
        // return
      }
    }
  }
  eprintln!("Program done!");
}


fn ensure_all_loaded(module: &Module, deps: &HashMap<Vec<String>, Module>) -> HashSet<Vec<String>> {
  let mut missing = HashSet::new();
  for dep in &module.dependencies {
    if dep != &module.name && !deps.contains_key(dep) {
      missing.insert(dep.clone());
    }
  }
  missing
}

fn format_module_name(name: &Vec<String>) -> String {
  name.join(".")
}

pub fn run(module: Module, deps: HashMap<Vec<String>, Module>) {
  eprintln!("Loading {:?}...", module.name);
  let missing_modules = ensure_all_loaded(&module, &deps);
  if missing_modules.len() > 0 {
    let missing_names = missing_modules
        .into_iter()
        .map(|m| format_module_name(&m))
        .collect::<Vec<String>>()
        .join(", ");
    panic!("Missing module(s): {}", missing_names);
  }
  run_main(module, deps);
}
