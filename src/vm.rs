use std::collections::BTreeMap as Map;
use std::collections::VecDeque;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "tag", content = "contents")]
enum Instruction {
  PushInt(i64),
  PushString(usize),
  LoadLocal(usize),
  StoreLocal(usize),
  LoadName(Vec<String>, String),
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

fn makeFrame(module: &Module, name: String) -> Frame {
  Frame {
    module: module,
    fun: name,
    ip: 0,
    base: 0,
    locals: vec!()
  }
}

fn curFn<'a>(module: &'a Module, fnName: String) -> &'a Vec<Instruction> {
  module.functions.get(&fnName).expect("No such fn")
}

#[derive(Debug)]
enum Value {
  IntVal(i64),
  StrVal(String),
  ModuleFnRef(Vec<String>, String),
}

// TODO we shouldn't have a single value type
struct GC(Vec<Value>); // TODO 2nd arena
#[derive(Clone, Copy)]
struct Ptr(usize); // todo thwarting ptr

impl GC {
  fn at(&self, i: Ptr) -> &Value {
    self.0.get(i.0).unwrap()
  }
  fn alloc(&mut self, v: Value) -> Ptr {
    self.0.push(v);
    Ptr(self.0.len() - 1)
  }
  fn new() -> Self {
    GC { 0: Vec::new() }
  }
}

fn goMain(module: Module) {
  let mut gc = GC::new();
  let mut stack: Vec<Ptr> = Vec::new();
  let mut frames: VecDeque<Frame> = VecDeque::new();
  frames.push_back(makeFrame(&module, "MAIN".to_string()));
  while frames.len() > 0 {
    let mut curFrame = frames.back_mut().unwrap();
    let fun = curFn(&module, curFrame.fun.to_string());
    println!("ip: {}", curFrame.ip);
    println!("got: {:?}", fun.get(curFrame.ip));
    match fun.get(curFrame.ip) {
      Some(Instruction::PushInt(n)) => {
        println!("Got push int");
        stack.push(gc.alloc(Value::IntVal(*n)));
        curFrame.ip += 1;
      },

      Some(Instruction::PushString(n)) => {
        println!("Got push int");
        let string = curFrame.module.strings.get(*n).expect("No such string");
        stack.push(gc.alloc(Value::StrVal(string.to_string())));
        curFrame.ip += 1;
      },

      Some(Instruction::LoadLocal(idx)) => {
        let ptr = curFrame.locals.get(*idx).expect("Trying to access uninitialized local");
        stack.push(*ptr);
        curFrame.ip += 1;
      },

      Some(Instruction::StoreLocal(idx)) => {
        let ptr = stack.pop().expect("Stack is empty, cannot store");
        if curFrame.locals.len() > *idx {
          curFrame.locals[*idx] = ptr;
        } else if curFrame.locals.len() == *idx {
          curFrame.locals.push(ptr);
        } else {
          panic!("Out-of-order local initialization!");
        }
        curFrame.ip += 1;
      }

      Some(Instruction::LoadName(namespace, name)) => {
        // this doesn't allow for peer deps, which is probably an issue:
        // If module A has [B] as deps, and B has [C] as deps,
        //   if B returns a function in C, then this here errors
        // This needs to be ruled out at semantic time, not here.
        // The VM should keep store of all its loaded modules somewhere
        if namespace[0] == "Prelude" || module.dependencies.contains(namespace) {
          stack.push(gc.alloc(Value::ModuleFnRef(namespace.clone(), name.clone())));
        } else {
          panic!("Trying to access to an un-loaded module");
        }
        curFrame.ip += 1;
      }

      Some(Instruction::LoadGlobal(name)) => {
        // TODO make sure the function exists
        stack.push(gc.alloc(Value::ModuleFnRef(curFrame.module.name.clone(), name.clone())));
        curFrame.ip += 1;
      },

      Some(Instruction::Jump(offset)) => {
        let ptr = stack.pop().expect("Nothing left on stack");
        let value = gc.at(ptr);
        curFrame.ip = *offset;
      },

      Some(Instruction::Unless(offset)) => {
        let ptr = stack.pop().expect("Nothing left on stack");
        let value = gc.at(ptr);
        match value {
          Value::IntVal(n) =>
              if *n == 0i64 {
                curFrame.ip = *offset
              } else {
                curFrame.ip += 1
              }
          _ =>
            curFrame.ip += 1
        }
      },

      Some(Instruction::Call(argNum)) => {
        // TODO need to think of a story for local functions and returning closures
        // one of the first thing we need is probably at semantic analysis stage. extract them to
        // be fake functions, and have an instruction to curry them, i.e.:
        // ModuleFnRefWithLocals([String], String, Locals: vec<Ptr>)
        let ptr = stack.pop().expect("Nothing left on stack to call");
        let value = gc.at(ptr);
        match value {
          Value::ModuleFnRef(ns, name) if ns[0] == "Prelude" => {
            for i in (1..=*argNum).rev() {
              println!("I/O {}: {:?}", i, gc.at(stack.pop().unwrap()));
            }
            curFrame.ip += 1;
          },
          Value::ModuleFnRef(ns, name) => {
            // NOTE: increment IP here, since adding a frame will invalid our borrow
            curFrame.ip += 1;
            // TODO args and stuff yada yada yada
            //      also, should probably reverse order of arguments
            let mut newFrame = makeFrame(&module, name.to_string());
            for i in (1..=*argNum).rev() {
              newFrame.locals.push(stack.pop().unwrap());
            }
            frames.push_back(newFrame);
          },
          _ => {
            panic!("Can't call!");
          }
        }
      },

      None => {
        if stack.len() < curFrame.base {
          panic!("Consumed too much stack space!");
        }
        while stack.len() > curFrame.base {
          stack.pop();
        }
        frames.pop_back().expect("No current frame?!");
        // return
      }
    }
  }
  println!("Program done!");
}

pub fn run(module: Module) {
  println!("Loading {:?}...", module.name);
  goMain(module);
}
