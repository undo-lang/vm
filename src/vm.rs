use crate::bc;
use crate::program::{link, ConstructorIndex, Context, FunctionIndex, Instruction, Program};
use std::{
    collections::VecDeque,
    fmt::{Display, Formatter},
    iter,
};

struct Frame {
    fn_idx: FunctionIndex,
    ip: usize,
    locals: Vec<Ptr>,
    registers: Vec<Option<Ptr>>,
    stack: Vec<Ptr>,
}

impl Frame {
    fn new(fn_idx: FunctionIndex) -> Self {
        Frame {
            fn_idx,
            ip: 0,
            locals: Vec::new(),
            registers: Vec::new(),
            stack: Vec::new(),
        }
    }
}

#[derive(Clone)]
enum Value {
    IntVal(i64),
    StrVal(String),
    ModuleFnRef(FunctionIndex),
    Intrinsic(String),
    VariantVal(ConstructorIndex, Vec<Ptr>),
    #[expect(unused)]
    LambdaVal(FunctionIndex, Vec<Ptr>),
    ThwartPtr(usize),
}

impl Display for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::IntVal(i) => write!(f, "{}", i),
            Value::StrVal(s) => write!(f, "{}", s),
            Value::ThwartPtr(_) => write!(f, "Thwart ptr"),
            _ => write!(f, "TODO Value"),
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
        Value::LambdaVal(fn_idx, mut ptrs) => {
            for ptr in &mut ptrs {
                compact_hit(old, new_arena, ptr);
            }
            new_arena.push(Value::LambdaVal(fn_idx, ptrs));
            old.set(ptr.0, Value::ThwartPtr(new_arena.len() - 1))
        }
        v => {
            new_arena.push(v.clone());
            old.set(ptr.0, Value::ThwartPtr(new_arena.len() - 1))
        }
    }
}

fn compact(mut old: GC, frames: &mut VecDeque<Frame>) -> GC {
    let mut new_arena: Vec<Value> = vec![];
    for frame in frames {
        for local in &mut frame.locals {
            compact_hit(&mut old, &mut new_arena, local);
        }
        for ptr in frame.stack.iter_mut() {
            compact_hit(&mut old, &mut new_arena, ptr);
        }
    }
    GC(new_arena)
}

impl GC {
    fn at(&self, i: Ptr) -> &Value {
        self.raw_at(i.0)
    }

    #[expect(unused)]
    fn at_mut(&mut self, i: Ptr) -> &Value {
        self.raw_at_mut(i.0)
    }

    fn raw_at(&self, i: usize) -> &Value {
        self.0.get(i).unwrap()
    }

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

fn err(msg: &'static str, cur_frame: &Frame, context: &Context) -> ! {
    panic!(
        "{} - {} ip={}",
        msg,
        context.fn_qualified_name(cur_frame.fn_idx),
        cur_frame.ip
    );
}

fn run_main(module_name: Vec<String>, program: Program, context: Context) {
    let mut num_frames = 0;
    let mut gc = GC::new();
    let mut frames: VecDeque<Frame> = VecDeque::new();

    let entrypoint_module = context
        .module_called(&module_name)
        .expect("Entrypoint module not loaded?");
    let entrypoint_fn = context
        .module_fn_called(entrypoint_module, "MAIN")
        .expect("MAIN not found");

    frames.push_back(Frame::new(entrypoint_fn));

    while !frames.is_empty() {
        num_frames = num_frames + 1;
        if num_frames == 500 {
            // TODO when near full or something...
            num_frames = 0;
            gc = compact(gc, &mut frames);
        }

        let cur_frame = frames.back_mut().unwrap();
        let fun = program.at(cur_frame.fn_idx);
        eprintln!(
            "ip: {} in {}",
            cur_frame.ip,
            context.fn_qualified_name(cur_frame.fn_idx)
        );

        match fun.get(cur_frame.ip) {
            Some(Instruction::PushInt(n)) => {
                cur_frame.stack.push(gc.alloc(Value::IntVal(*n)));
                cur_frame.ip += 1;
            }

            Some(Instruction::PushString(n)) => {
                let string = context.string(*n);
                cur_frame
                    .stack
                    .push(gc.alloc(Value::StrVal(string.clone())));
                cur_frame.ip += 1;
            }

            Some(Instruction::LoadLocal(idx)) => {
                let ptr = cur_frame
                    .locals
                    .get(*idx)
                    .expect("Trying to access uninitialized local");
                cur_frame.stack.push(*ptr);
                cur_frame.ip += 1;
            }

            Some(Instruction::StoreLocal(idx)) => {
                let ptr = cur_frame.stack.pop().expect("Stack is empty, cannot store");
                if cur_frame.locals.len() > *idx {
                    cur_frame.locals[*idx] = ptr;
                } else if cur_frame.locals.len() == *idx {
                    cur_frame.locals.push(ptr);
                } else {
                    panic!("Out-of-order local initialization!");
                }
                cur_frame.ip += 1;
            }

            Some(Instruction::LoadReg(idx)) => {
                let ptr = cur_frame
                    .registers
                    .get(*idx)
                    .expect("Register not allocated")
                    .expect("Register empty");
                cur_frame.stack.push(ptr);
                cur_frame.ip += 1;
            }

            Some(Instruction::StoreReg(idx)) => {
                let ptr = cur_frame.stack.pop().expect("Stack is empty, cannot store");
                if cur_frame.registers.len() <= *idx {
                    cur_frame.registers.resize(*idx + 1, None);
                }
                cur_frame.registers[*idx] = Some(ptr);
                cur_frame.ip += 1;
            }

            Some(Instruction::LoadName(fn_idx)) => {
                cur_frame.stack.push(gc.alloc(Value::ModuleFnRef(*fn_idx)));
                cur_frame.ip += 1;
            }

            Some(Instruction::LoadIntrinsic(intr)) => {
                cur_frame
                    .stack
                    .push(gc.alloc(Value::Intrinsic(intr.clone())));
                cur_frame.ip += 1;
            }

            Some(Instruction::Jump(offset)) => {
                cur_frame.ip = *offset;
            }

            Some(Instruction::Unless(offset)) => {
                let Some(ptr) = cur_frame.stack.pop() else {
                    err("`unless` - stack exhaustion", cur_frame, &context);
                };
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
                let Some(ptr) = cur_frame
                    .stack
                    .pop() else {
                    err("`call` - callee exhaustion", cur_frame, &context);
                };
                let value = gc.at(ptr);
                match value {
                    Value::Intrinsic(name) => {
                        match name.as_str() {
                            "print" => {
                                for _ in 1..=*arg_num {
                                    println!("{}", gc.at(cur_frame.stack.pop().unwrap()));
                                }
                            }
                            "+" => define_arithmetic_operator!(+, gc, cur_frame.stack, arg_num),
                            "-" => define_arithmetic_operator!(-, gc, cur_frame.stack, arg_num),
                            "/" => define_arithmetic_operator!(/, gc, cur_frame.stack, arg_num),
                            "*" => define_arithmetic_operator!(*, gc, cur_frame.stack, arg_num),
                            ">" => define_boolean_operator!(>, gc, cur_frame.stack, arg_num),
                            "<" => define_boolean_operator!(<, gc, cur_frame.stack, arg_num),
                            "==" => define_boolean_operator!(==, gc, cur_frame.stack, arg_num),
                            ">=" => define_boolean_operator!(>=, gc, cur_frame.stack, arg_num),
                            "<=" => define_boolean_operator!(<=, gc, cur_frame.stack, arg_num),
                            "!=" => define_boolean_operator!(!=, gc, cur_frame.stack, arg_num),
                            // TODO ++
                            _ => panic!("No such prelude fn: {name}", name = name),
                        }
                        cur_frame.ip += 1;
                    }

                    Value::ModuleFnRef(fn_idx) => {
                        // NOTE: increment IP here, since adding a frame will invalidate our borrow
                        cur_frame.ip += 1;
                        let mut new_frame = Frame::new(*fn_idx);
                        for _ in 1..=*arg_num {
                            new_frame.locals.push(cur_frame.stack.pop().unwrap());
                        }
                        new_frame.locals.reverse(); // arg0=local0, etc
                        frames.push_back(new_frame);
                    }

                    _ => {
                        err("`call` - not a callable", cur_frame, &context);
                    }
                }
            }

            Some(Instruction::Instantiate(ctor_idx)) => {
                let nbr = context.ctor_fields_nbr(*ctor_idx);
                let els = iter::repeat(0)
                    .take(nbr)
                    .map(|_| cur_frame.stack.pop().unwrap())
                    .rev()
                    .collect();
                cur_frame
                    .stack
                    .push(gc.alloc(Value::VariantVal(*ctor_idx, els)));
                cur_frame.ip += 1;
            }

            Some(Instruction::IsVariant(ctor)) => {
                let val = gc.at(cur_frame.stack.pop().unwrap());
                match val {
                    Value::VariantVal(vc, _) => {
                        let ret = if vc == ctor { 1i64 } else { 0i64 };
                        cur_frame.stack.push(gc.alloc(Value::IntVal(ret)));
                        cur_frame.ip += 1;
                    }
                    _ => {
                        err(
                            "`is_variant` - Cannot check variant of a non-ADT",
                            cur_frame,
                            &context,
                        );
                    }
                }
            }
            Some(Instruction::Field(ctor, i)) => match gc.at(cur_frame.stack.pop().unwrap()) {
                Value::VariantVal(vc, ptrs) => {
                    if ctor != vc {
                        panic!(
                            "Expected variant {}, got {} in field access",
                            context.ctor_qualified_name(*ctor),
                            context.ctor_qualified_name(*vc),
                        );
                    }
                    cur_frame.stack.push(ptrs[*i]);
                    cur_frame.ip += 1;
                }
                _ => {
                    err(
                        "`field` - Cannot access field of non-ADT",
                        cur_frame,
                        &context,
                    );
                }
            },

            None => {
                let old_frame = frames.pop_back().expect("No current frame?!");
                let Some(outer) = frames.back_mut() else {
                    if old_frame.stack.is_empty() {
                        continue;
                    }
                    panic!(
                        "Top-level {} leaked values!",
                        context.fn_qualified_name(old_frame.fn_idx)
                    );
                };
                match old_frame.stack[..] {
                    [] => {}
                    [o] => {
                        outer.stack.insert(0, o);
                    }
                    _ => {
                        panic!(
                            "{} leaked values!",
                            context.fn_qualified_name(old_frame.fn_idx)
                        );
                    }
                }
            }
        }
    }
    eprintln!("Program done!");
}

pub fn run(module: Vec<String>, modules: Vec<bc::Module>) {
    eprintln!("Running {:?}...", module);
    let (program, context) = link(&modules);
    run_main(module, program, context);
}
