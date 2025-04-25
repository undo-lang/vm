use std::collections::{HashMap, HashSet};
use std::ops::Index;
use crate::bc;
use crate::bc::ModuleName;

pub struct Program {
    functions: Vec<Vec<Instruction>>,
}

pub struct Context<'a> {
    // module idx -> module name
    module_names: Vec<&'a Vec<String>>,
    // function idx -> module idx
    function_modules: Vec<usize>,
    // function idx -> module name
    function_module_names: Vec<&'a Vec<String>>,
    // function idx -> function name
    function_names: Vec<&'a String>,
    // datatype_modules: Vec<&'a Vec<String>>
    // string table idx -> string
    strings: Vec<&'a String>,
}

fn check_modules(modules: &Vec<bc::Module>) {
    let all_dependencies: HashSet<&Vec<String>> = modules.iter().flat_map(|m| &m.dependencies).collect();
    let provided_modules = modules.iter().map(|m| &m.name).collect::<HashSet<_>>();
    if provided_modules != all_dependencies {
        let missing = provided_modules.difference(&all_dependencies);
        let extra = all_dependencies.difference(&provided_modules);
        let missing_str = missing.map(|v| v.join("::")).collect::<Vec<String>>().join(", ");
        let extra_str = extra.map(|v| v.join("::")).collect::<Vec<String>>().join(", ");
        if missing_str.is_empty() {
            panic!("Extra modules provided: {}.", extra_str);
        } else if extra_str.is_empty() {
            panic!("Dependencies not matched: {}.", missing_str);
        } else {
            panic!("Dependencies mismatch, missing {} but provided {}", missing_str, extra_str);
        }
    }
}

pub fn link(modules: &Vec<bc::Module>) -> (Program, Context) {
    check_modules(&modules);

    let mut function_modules: Vec<usize> = vec![];
    let mut function_module_names: Vec<&Vec<String>> = vec![];
    let mut function_names: Vec<&String> = vec![];
    // XXX we could dedupe strings
    let strings: Vec<&String> = modules.iter().flat_map(|m| &m.strings).collect();

    let module_names = modules.iter().map(|m| &m.name).collect();
    let mut module_function_mapping = HashMap::new();

    for (m_idx, module) in modules.iter().enumerate() {
        let mut module_fns = HashMap::new();
        for fn_name in module.functions.keys() {
            let f_idx = function_names.len();
            function_modules.push(m_idx);
            function_module_names.push(&module.name);
            function_names.push(fn_name);
            module_fns.insert(fn_name, f_idx);
        }
        module_function_mapping.insert(m_idx, module_fns);
    }

    let context = Context {
        module_names,
        function_modules,
        function_module_names,
        function_names,
        strings,
    };

    let functions= modules.iter()
        .enumerate()
        .flat_map(|(m_idx, m)|
            m.functions.values()
                .map(|f| (m_idx, f))
                .collect::<Vec<_>>()
        )
        .enumerate()
        .map(|(f_idx, (m_idx, f))| compile(m_idx, f_idx, f, &context))
        .collect::<Vec<_>>();

    assert_eq!(functions.len(), context.function_modules.len());
    assert_eq!(functions.len(), context.function_module_names.len());
    assert_eq!(functions.len(), context.function_names.len());

    let program = Program { functions };
    (program, context)
}

fn compile(cur_module_idx: usize, fn_idx: usize, instrs: &Vec<bc::RawInstruction>, context: &Context) -> Vec<Instruction> {
    use bc::RawInstruction;
    instrs.iter().map(|instr| match instr {
        RawInstruction::PushInt(i) => Instruction::PushInt(*i),
        RawInstruction::PushString(idx) => {
            // TODO this is incorrect since we merged string table
            //      maybe just get rid of string tables?
            Instruction::PushString(StringTableIndex(*idx))
        },
        RawInstruction::LoadLocal(i) => Instruction::LoadLocal(*i),
        RawInstruction::StoreLocal(i) => Instruction::StoreLocal(*i),
        RawInstruction::Unless(i) => Instruction::Unless(*i),
        RawInstruction::Jump(i) => Instruction::Jump(*i),
        RawInstruction::Call(i) => Instruction::Call(*i),
        RawInstruction::LoadName(ModuleName { module }, fun ) => {
            let fn_idx = context.function_module_names.iter()
                .zip(&context.function_names)
                .position(|(m_name, &fn_name)| module == *m_name && fun == fn_name)
                .unwrap();
            Instruction::LoadName(FunctionIndex(fn_idx))
        }
        RawInstruction::LoadGlobal(fun ) => {
            let fn_idx = context.function_modules.iter()
                .zip(&context.function_names)
                .position(|(m_idx, &fn_name)| cur_module_idx == *m_idx && fun == fn_name)
                .unwrap();
            Instruction::LoadName(FunctionIndex(fn_idx))
        }
        RawInstruction::Instantiate(_, _, _) => panic!()
    }).collect()
}

pub struct ModuleIndex(usize);
pub struct FunctionIndex(usize);
pub struct StringTableIndex(usize);
pub struct ADTIndex(usize);
pub struct ConstructorIndex(usize);
pub enum Instruction {
    PushInt(i64),
    PushString(StringTableIndex),
    LoadLocal(usize),
    StoreLocal(usize),
    Unless(usize),
    Jump(usize),
    Call(usize),
    LoadName(FunctionIndex),
    Instantiate(ConstructorIndex),
}