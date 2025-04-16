use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::collections::{BTreeMap as Map, HashMap, HashSet};

#[derive(Serialize, Deserialize, Debug)]
#[serde()]
struct ModuleName {
    module: Vec<String>,
}

fn is_prelude_(module_name: &Vec<String>) -> bool {
    module_name.len() == 1 && module_name[0] == "Prelude"
}

fn is_prelude(module_name: &ModuleName) -> bool {
    is_prelude_(&module_name.module)
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
    pub name: Vec<String>,
    strings: Vec<String>,
    functions: Map<String, Vec<Instruction>>,
    dependencies: Vec<Vec<String>>,
}

struct Frame<'a> {
    module: &'a Module,
    fun: String,
    ip: usize,
    locals: Vec<Ptr>, // XXX we'll want to serialize this when we store closures
                      //     this will prevent captures from being gc'd
}

// TODO impl
fn make_frame(module: &Module, name: String) -> Frame {
    Frame {
        module,
        fun: name,
        ip: 0,
        locals: vec![],
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
    VariantVal(usize, Vec<Ptr>),
}

impl Value {
    fn to_string(&self) -> String {
        match self {
            Value::IntVal(i) => i.to_string(),
            Value::StrVal(s) => s.to_string(),
            Value::ModuleFnRef(m, f) => "{0}::{1}".format(m.join("::"), f.to_string()),
            Value::ThwartPtr(_) => "Thwart ptr".to_string(),
            Value::VariantVal(i, s) => {
                "{0}({1})".format(i, s.iter().map(|v| v.to_string()).collect().join(", "))
            }
        }
    }
}

impl Clone for Value {
    fn clone(&self) -> Value {
        match self {
            Value::IntVal(i) => Value::IntVal(*i),
            Value::StrVal(s) => Value::StrVal(s.to_string()),
            Value::ModuleFnRef(ns, f) => {
                Value::ModuleFnRef(ns.iter().map(|s| s.to_string()).collect(), f.to_string())
            }
            Value::ThwartPtr(i) => Value::ThwartPtr(*i),
            Value::VariantVal(i, ptrs) => Value::VariantVal(*i, ptrs.to_vec()),
        }
    }
}

// TODO we shouldn't have a single value type
struct GC(Vec<Value>);

#[derive(Clone, Copy)]
struct Ptr(usize); //, usize);

fn compact_hit(old: &mut GC, new_arena: &mut Vec<Value>, ptr: &Ptr) -> usize {
    match old.raw_at(ptr.0) {
        Value::ThwartPtr(i) => *i, // Rewrite ptr
        Value::VariantVal(i, mut ptrs) => {
            let news = ptrs
                .iter()
                .map(|ptr| Ptr(compact_hit(old, new_arena, ptr)))
                .collect();
            new_arena.push(Value::VariantVal(*i, news));
            new_arena.len() - 1
        }
        v => {
            new_arena.push(v.clone());
            new_arena.len() - 1
        }
    }
}

fn compact(mut old: GC, frames: &mut VecDeque<Frame>, mut stack: &Vec<Ptr>) -> GC {
    let mut new_arena: Vec<Value> = vec![];
    for ptr in stack.iter_mut() {
        ptr.0 = compact_hit(&mut old, &mut new_arena, ptr);
    }
    for frame in frames {
        for local in &mut frame.locals {
            local.0 = compact_hit(&mut old, &mut new_arena, local);
        }
    }
    GC(new_arena)
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

macro_rules! define_arithmetic_operator {
    ( $op:tt, $gc:expr, $stack:expr, $arg_num:expr ) => {
        {
            let mut result: i64 = match $gc.at($stack.pop().unwrap()) {
                Value::IntVal(val) => *val,
                _ => panic!("Cannot use that operator on a non-int")
            };
            let mut i: usize = 1; // Start at 1, we already handled the first
            while &i < $arg_num {
                match $gc.at($stack.pop().unwrap()) {
                    Value::IntVal(val) => result = result $op val,
                    _ => panic!("Cannot perform arithmetic on a non-int value")
                }
                i += 1;
            }
            $stack.push($gc.alloc(Value::IntVal(result)))
        }
    }
}

fn run_main(module_name: Vec<String>, modules: HashMap<Vec<String>, Module>) {
    let mut gc = GC::new();
    let mut stack: Vec<Ptr> = Vec::new();
    let mut frames: VecDeque<Frame> = VecDeque::new();
    let entrypoint_module: &Module = modules.get(&module_name).unwrap();
    frames.push_back(make_frame(&entrypoint_module, "MAIN".to_string()));

    while !frames.is_empty() {
        let mut cur_frame = frames.back_mut().unwrap();
        let fun = cur_fn(&cur_frame.module, cur_frame.fun.to_string());
        eprintln!("ip: {}", cur_frame.ip);
        eprintln!("got: {:?}", fun.get(cur_frame.ip));

        match fun.get(cur_frame.ip) {
            Some(Instruction::PushInt(n)) => {
                stack.push(gc.alloc(Value::IntVal(*n)));
                cur_frame.ip += 1;
            }

            Some(Instruction::PushString(n)) => {
                let string = cur_frame.module.strings.get(*n).expect("No such string");
                stack.push(gc.alloc(Value::StrVal(string.to_string())));
                cur_frame.ip += 1;
            }

            Some(Instruction::LoadLocal(idx)) => {
                let ptr = cur_frame
                    .locals
                    .get(*idx)
                    .expect("Trying to access uninitialized local");
                stack.push(*ptr);
                cur_frame.ip += 1;
            }

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
                if is_prelude(namespace) || modules.contains_key(&namespace.module) {
                    stack
                        .push(gc.alloc(Value::ModuleFnRef(namespace.module.clone(), name.clone())));
                } else {
                    eprintln!("Wrong module: {:?}", namespace);
                    panic!("Trying to access to an un-loaded/unprovided module");
                }
                cur_frame.ip += 1;
            }

            Some(Instruction::LoadGlobal(name)) => {
                // TODO make sure the function exists
                stack.push(gc.alloc(Value::ModuleFnRef(
                    cur_frame.module.name.clone(),
                    name.clone(),
                )));
                cur_frame.ip += 1;
            }

            Some(Instruction::Jump(offset)) => {
                cur_frame.ip = *offset;
            }

            Some(Instruction::Unless(offset)) => {
                let ptr = stack.pop().expect("Nothing left on stack");
                let value = gc.at(ptr);
                match value {
                    Value::IntVal(n) => {
                        if *n == 0i64 {
                            cur_frame.ip = *offset
                        } else {
                            cur_frame.ip += 1
                        }
                    }
                    _ => cur_frame.ip += 1,
                }
            }

            Some(Instruction::Call(arg_num)) => {
                // TODO need to think of a story for local functions and returning closures
                // one of the first thing we need is probably at semantic analysis stage. extract them to
                // be fake functions, and have an instruction to curry them, i.e.:
                // ModuleFnRefWithLocals([String], String, Locals: vec<Ptr>)
                let ptr = stack.pop().expect("Nothing left on stack to call");
                let value = gc.at(ptr);
                match value {
                    Value::ModuleFnRef(ns, name) if is_prelude_(&ns) => {
                        match name.as_str() {
                            "print" => {
                                for _ in 1..=*arg_num {
                                    println!("{}", gc.at(stack.pop().unwrap()).to_string());
                                }
                            }
                            "+" => define_arithmetic_operator!(+, gc, stack, arg_num),
                            "-" => define_arithmetic_operator!(-, gc, stack, arg_num),
                            "/" => define_arithmetic_operator!(/, gc, stack, arg_num),
                            "*" => define_arithmetic_operator!(*, gc, stack, arg_num),
                            ">" => define_arithmetic_operator!(>, gc, stack, arg_num),
                            "<" => define_arithmetic_operator!(<, gc, stack, arg_num),
                            "==" => define_arithmetic_operator!(==, gc, stack, arg_num),
                            ">=" => define_arithmetic_operator!(>=, gc, stack, arg_num),
                            "<=" => define_arithmetic_operator!(<=, gc, stack, arg_num),
                            "!=" => define_arithmetic_operator!(!=, gc, stack, arg_num),
                            // TODO ++
                            _ => panic!("No such prelude fn: {name}", name = name),
                        }
                        cur_frame.ip += 1;
                    }

                    Value::ModuleFnRef(ns, name) => {
                        // NOTE: increment IP here, since adding a frame will invalidate our borrow
                        cur_frame.ip += 1;
                        let mut new_frame = make_frame(modules.get(ns).unwrap(), name.to_string());
                        // Reverse arguments because we push(pop())
                        for _ in (1..=*arg_num).rev() {
                            new_frame.locals.push(stack.pop().unwrap());
                        }
                        frames.push_back(new_frame);
                    }
                    _ => {
                        panic!("Can't call!");
                    }
                }
            }

            None => {
                // TODO reinstate some sort of %bsp?

                frames.pop_back().expect("No current frame?!");
            }
        }
    }
    eprintln!("Program done!");
}

fn ensure_all_loaded(modules: &HashMap<Vec<String>, Module>) -> HashSet<Vec<String>> {
    let mut bfs: Vec<Vec<String>> = modules
        .keys()
        .into_iter()
        .map(|name| name.clone())
        .collect();
    let mut seen: HashSet<Vec<String>> = HashSet::new();
    let mut missing = HashSet::new();
    while !bfs.is_empty() {
        // TODO while pop
        let item: Vec<String> = bfs.pop().unwrap();
        match modules.get(&item) {
            Some(&ref module) => {
                // Mark current module as seen
                seen.insert(item.clone());
                // Traverse all deps, add them to the BFS if we haven't seen them already
                for dep in &module.dependencies {
                    if !seen.contains(dep) {
                        bfs.push(dep.to_vec());
                    }
                }
            }
            None => {
                // No dynamic module loading, if it's not in `deps` it's missing
                missing.insert(item.clone());
            }
        }
    }
    missing
}

fn format_module_name(name: &Vec<String>) -> String {
    name.join(".")
}

pub fn run(module: Vec<String>, modules: HashMap<Vec<String>, Module>) {
    let missing_modules = ensure_all_loaded(&modules);
    if !missing_modules.is_empty() {
        let missing_names = missing_modules
            .into_iter()
            .map(|m| format_module_name(&m))
            .collect::<Vec<String>>()
            .join(", ");
        panic!("Missing module(s): {}", missing_names);
    }
    eprintln!("Running {:?}...", module);
    run_main(module, modules);
}
