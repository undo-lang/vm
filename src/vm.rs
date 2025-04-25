use crate::context::{build_context, Context};
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, collections::{HashMap, HashSet}, fmt::{Debug, Display, Formatter}, iter};

fn is_prelude_(module_name: &[String]) -> bool {
    module_name.len() == 1 && module_name[0] == "Prelude"
}

fn is_prelude(module_name: &ModuleName) -> bool {
    is_prelude_(&module_name.module)
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
    VariantVal(usize, Vec<Ptr>),
    #[expect(unused)]
    LambdaVal(Vec<String>, String, Vec<Ptr>),
    ThwartPtr(usize),
}

impl Display for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::IntVal(i) => write!(f, "{}", i),
            Value::StrVal(s) => write!(f, "{}", s),
            Value::ModuleFnRef(m, fnm) => write!(f, "{0}::{1}", m.join("::"), fnm),
            Value::VariantVal(i, s) => {
                write!(f, "{0}(#{1} args)", i, s.len())
            }
            Value::LambdaVal(m, fnm, _) => write!(f, "(LAMBDA {0}::{1})", m.join("::"), fnm),
            Value::ThwartPtr(_) => write!(f, "Thwart ptr"),
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
            Value::VariantVal(i, ptrs) => Value::VariantVal(*i, ptrs.to_vec()),
            Value::LambdaVal(ns, f, ptrs) => Value::LambdaVal(
                ns.iter().map(|s| s.to_string()).collect(),
                f.to_string(),
                ptrs.to_vec(),
            ),
            Value::ThwartPtr(i) => Value::ThwartPtr(*i),
        }
    }
}

// TODO we shouldn't have a single value type
struct GC(Vec<Value>);

#[derive(Clone, Copy)]
struct Ptr(usize); //, usize);

fn compact_hit(old: &mut GC, new_arena: &mut Vec<Value>, ptr: &mut Ptr) {
    match old.raw_at(ptr.0).clone() {
        Value::ThwartPtr(i) => ptr.0 = i, // Rewrite ptr
        Value::VariantVal(i, mut ptrs) => {
            for ptr in &mut ptrs {
                compact_hit(old, new_arena, ptr);
            }
            new_arena.push(Value::VariantVal(i, ptrs));
            old.set(ptr.0, Value::ThwartPtr(new_arena.len() - 1))
        }
        Value::LambdaVal(module, fnm, mut ptrs) => {
            for ptr in &mut ptrs {
                compact_hit(old, new_arena, ptr);
            }
            new_arena.push(Value::LambdaVal(module, fnm, ptrs));
            old.set(ptr.0, Value::ThwartPtr(new_arena.len() - 1))
        }
        v => {
            new_arena.push(v.clone());
            old.set(ptr.0, Value::ThwartPtr(new_arena.len() - 1))
        }
    }
}

fn compact(mut old: GC, frames: &mut VecDeque<Frame>, stack: &mut [Ptr]) -> GC {
    let mut new_arena: Vec<Value> = vec![];
    for ptr in stack.iter_mut() {
        compact_hit(&mut old, &mut new_arena, ptr);
    }
    for frame in frames {
        for local in &mut frame.locals {
            compact_hit(&mut old, &mut new_arena, local);
        }
    }
    GC(new_arena)
}

impl GC {
    fn at(&mut self, i: Ptr) -> &Value {
        self.raw_at(i.0)
    }

    fn raw_at(&self, i: usize) -> &Value {
        self.0.get(i).unwrap()
    }

    #[expect(unused)]
    fn raw_at_mut(&mut self, i: usize) -> &mut Value {
        self.0.get_mut(i).unwrap()
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
        GC(Vec::new())
    }
}

macro_rules! define_arithmetic_operator {
    ( $op:tt, $gc:expr, $stack:expr, $arg_num:expr ) => {
        {
            let mut result: i64 = match $gc.at($stack.pop().unwrap()) {
                Value::IntVal(val) => *val,
                _ => panic!("Cannot use {} on a non-int", stringify!($op))
            };
            let mut i: usize = 1; // Start at 1, we already handled the first
            while &i < $arg_num {
                match $gc.at($stack.pop().unwrap()) {
                    Value::IntVal(val) => result = result $op val,
                    _ => panic!("Cannot use {} on a non-int value", stringify!($op))
                }
                i += 1;
            }
            $stack.push($gc.alloc(Value::IntVal(result)))
        }
    }
}
macro_rules! define_boolean_operator {
    ( $op:tt, $gc:expr, $stack:expr, $arg_num:expr ) => {
        {
            if *$arg_num != 2usize {
                panic!("non-binary-applied boolean exprs TODO")
            }
            let fst: i64 = match $gc.at($stack.pop().unwrap()) {
                Value::IntVal(val) => *val,
                _ => panic!("Cannot use {} on a non-int", stringify!($op))
            };
            let snd: i64 = match $gc.at($stack.pop().unwrap()) {
                Value::IntVal(val) => *val,
                _ => panic!("Cannot use {} on a non-int", stringify!($op))
            };
            // TODO bool
            let result: i64 = (fst $op snd) as i64;
            $stack.push($gc.alloc(Value::IntVal(result)))
        }
    }
}

fn run_main(module_name: Vec<String>, modules: &HashMap<Vec<String>, Module>, context: Context) {
    let mut num_frames = 0;
    let mut gc = GC::new();
    let mut frames: VecDeque<Frame> = VecDeque::new();
    let mut stack: Vec<Ptr> = Vec::new();
    let entrypoint_module: &Module = modules.get(&module_name).unwrap();
    frames.push_back(make_frame(entrypoint_module, "MAIN".to_string()));

    while !frames.is_empty() {
        num_frames = num_frames + 1;
        if num_frames == 500 {
            // TODO when near full or something...
            num_frames = 0;
            gc = compact(gc, &mut frames, &mut stack);
        }

        let cur_frame = frames.back_mut().unwrap();
        let fun = cur_fn(cur_frame.module, cur_frame.fun.to_string());
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
                    Value::ModuleFnRef(ns, name) if is_prelude_(ns) => {
                        match name.as_str() {
                            "print" => {
                                for _ in 1..=*arg_num {
                                    println!("{}", gc.at(stack.pop().unwrap()));
                                }
                            }
                            "+" => define_arithmetic_operator!(+, gc, stack, arg_num),
                            "-" => define_arithmetic_operator!(-, gc, stack, arg_num),
                            "/" => define_arithmetic_operator!(/, gc, stack, arg_num),
                            "*" => define_arithmetic_operator!(*, gc, stack, arg_num),
                            ">" => define_boolean_operator!(>, gc, stack, arg_num),
                            "<" => define_boolean_operator!(<, gc, stack, arg_num),
                            "==" => define_boolean_operator!(==, gc, stack, arg_num),
                            ">=" => define_boolean_operator!(>=, gc, stack, arg_num),
                            "<=" => define_boolean_operator!(<=, gc, stack, arg_num),
                            "!=" => define_boolean_operator!(!=, gc, stack, arg_num),
                            // TODO ++
                            _ => panic!("No such prelude fn: {name}", name = name),
                        }
                        cur_frame.ip += 1;
                    }

                    Value::ModuleFnRef(ns, name) => {
                        // NOTE: increment IP here, since adding a frame will invalidate our borrow
                        cur_frame.ip += 1;
                        let mut new_frame = make_frame(modules.get(ns).unwrap(), name.to_string());
                        // TODO reverse args?
                        for _ in 1..=*arg_num {
                            new_frame.locals.push(stack.pop().unwrap());
                        }
                        frames.push_back(new_frame);
                    }
                    _ => {
                        panic!("Tried to invoke a non-callable");
                    }
                }
            }
            Some(Instruction::Instantiate(module, adt, ctor)) => {
                let data = context
                    .adts
                    .get(&module.module)
                    .and_then(|xs| xs.get(adt))
                    .and_then(|xs| xs.get(ctor))
                    .expect("ICE: ADT doesn't exist");
                let els = iter::repeat(0).take(data.elements).map(|_| stack.pop().unwrap()).collect();
                stack.push(gc.alloc(Value::VariantVal(data.id, els)));
            }

            None => {
                // TODO reinstate some sort of %bsp?

                frames.pop_back().expect("No current frame?!");
            }
        }
    }
    eprintln!("Program done!");
}

// TODO pass (dependencies) instead of `Module` so it can be moved to Context
fn ensure_all_loaded(modules: &HashMap<Vec<String>, Module>) -> HashSet<Vec<String>> {
    let mut bfs: Vec<Vec<String>> = modules.keys().cloned().collect();
    let mut seen: HashSet<Vec<String>> = HashSet::new();
    let mut missing = HashSet::new();
    while let Some(item) = bfs.pop() {
        match modules.get(&item) {
            Some(module) => {
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

fn format_module_name(name: &[String]) -> String {
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
    let context = build_context(&modules);
    run_main(module, &modules, context);
}
