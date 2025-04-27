use crate::bc;
use crate::bc::ModuleName;
use std::collections::HashSet;

pub struct Program {
    functions: Vec<Vec<Instruction>>,
}

impl Program {
    pub fn at(&self, FunctionIndex(i): FunctionIndex) -> &Vec<Instruction> {
        &self.functions[i]
    }
}

pub struct Context<'a> {
    // module idx -> module name
    module_names: Vec<&'a Vec<String>>,

    // function idx -> module idx
    function_modules: Vec<ModuleIndex>,
    // function idx -> module name
    function_module_names: Vec<&'a Vec<String>>,
    // function idx -> function name
    function_names: Vec<&'a String>,

    // datatype -> module idx
    datatype_modules: Vec<ModuleIndex>,
    // datatype -> module name
    datatype_module_names: Vec<&'a Vec<String>>,
    // datatype -> datatype name
    datatype_names: Vec<&'a String>,

    // constructor -> module idx
    constructor_modules: Vec<ModuleIndex>,
    // constructor -> module name
    constructor_module_names: Vec<&'a Vec<String>>,
    // constructor -> datatype idx
    constructor_datatypes: Vec<DatatypeIndex>,
    // constructor -> datatype name
    constructor_datatype_names: Vec<&'a String>,
    // constructor -> constructor name
    constructor_names: Vec<&'a String>,
    // constructor -> constructor field names
    constructor_fields: Vec<&'a Vec<String>>,

    // string table idx -> string
    // XXX HashMap<usize, Vec<&'a String>>? + LoadString(usize, usize)
    strings: Vec<&'a String>,
}

impl<'a> Context<'a> {
    // Module-related functions
    pub fn module_called(&'a self, name: &Vec<String>) -> Option<ModuleIndex> {
        self.module_names
            .iter()
            .position(|&m| m == name)
            .map(|m| ModuleIndex(m))
    }

    pub fn module_fn_called(
        &'a self,
        module: ModuleIndex,
        name: &'static str,
    ) -> Option<FunctionIndex> {
        self.function_modules
            .iter()
            .zip(&self.function_names)
            .position(|(&m, &n)| m == module && n == name)
            .map(|i| FunctionIndex(i))
    }

    // Function-related functions
    pub fn qualified_name(&'a self, FunctionIndex(i): FunctionIndex) -> String {
        assert!(i < self.function_names.len());
        format!(
            "{}::{}",
            self.function_module_names[i].join("::"),
            self.function_names[i]
        )
    }

    // Datatype-related functions

    // Constructor-related functions
    pub fn ctor_fields_nbr(&self, ConstructorIndex(i): ConstructorIndex) -> usize {
        assert!(i < self.constructor_fields.len());
        self.constructor_fields[i].len()
    }

    // Strings-related functions
    pub fn string(&self, StringTableIndex(i): StringTableIndex) -> &String {
        self.strings[i]
    }
}

fn check_modules(modules: &Vec<bc::Module>) {
    let all_dependencies: HashSet<&Vec<String>> =
        modules.iter().flat_map(|m| &m.dependencies).collect();
    let provided_modules = modules.iter().map(|m| &m.name).collect::<HashSet<_>>();
    let missing = all_dependencies
        .difference(&provided_modules)
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        let missing_str = missing
            .iter()
            .map(|v| v.join("::"))
            .collect::<Vec<String>>()
            .join(", ");
        let provided_modules_str = provided_modules
            .iter()
            .map(|v| v.join("::"))
            .collect::<Vec<String>>()
            .join(", ");
        panic!(
            "Dependencies mismatch, missing {} but provided {}",
            missing_str, provided_modules_str
        );
    }

    // XXX check ADTs refers to existing modules too
}

fn is_intrinsic(n: &String) -> bool {
    n == "print" || n == "+"
}

pub fn link(modules: &Vec<bc::Module>) -> (Program, Context) {
    check_modules(&modules);

    let mut context = Context {
        module_names: modules.iter().map(|m| &m.name).collect(),
        function_modules: Vec::new(),
        function_module_names: Vec::new(),
        function_names: Vec::new(),
        datatype_modules: Vec::new(),
        datatype_module_names: Vec::new(),
        datatype_names: Vec::new(),
        constructor_modules: Vec::new(),
        constructor_module_names: Vec::new(),
        constructor_datatypes: Vec::new(),
        constructor_datatype_names: Vec::new(),
        constructor_names: Vec::new(),
        constructor_fields: Vec::new(),
        strings: modules.iter().flat_map(|m| &m.strings).collect(),
    };

    // let mut module_function_mapping = HashMap::new();

    for (m_idx_raw, module) in modules.iter().enumerate() {
        // let mut module_fns = HashMap::new();
        let m_idx = ModuleIndex(m_idx_raw);
        let mut fn_keys = module.functions.keys().collect::<Vec<_>>();
        fn_keys.sort();
        for fn_name in fn_keys {
            // let f_idx = function_names.len();
            context.function_modules.push(m_idx);
            context.function_module_names.push(&module.name);
            context.function_names.push(fn_name);
            // module_fns.insert(fn_name, f_idx);
        }
        // module_function_mapping.insert(m_idx, module_fns);

        for (datatype_name, ctors) in module.adts.iter() {
            let datatype_idx = DatatypeIndex(context.datatype_modules.len());
            context.datatype_modules.push(m_idx);
            context.datatype_module_names.push(&module.name);
            context.datatype_names.push(datatype_name);

            for ctor in ctors {
                context.constructor_modules.push(m_idx);
                context.constructor_module_names.push(&module.name);
                context.constructor_datatypes.push(datatype_idx);
                context.constructor_datatype_names.push(datatype_name);
                context.constructor_names.push(&ctor.name);
                context.constructor_fields.push(&ctor.elements);
            }
        }
    }

    let functions = modules
        .iter()
        .enumerate()
        .flat_map(|(m_idx, m)| {
            let mut fns = m.functions.iter().collect::<Vec<_>>();
            fns.sort_by_key(|(f, _)| *f);
            fns.iter().map(|f| (m_idx, f.1)).collect::<Vec<_>>()
        })
        .enumerate()
        .map(|(f_idx, (m_idx, f))| compile(ModuleIndex(m_idx), f_idx, f, &context))
        .collect::<Vec<_>>();

    // Sanity checks
    assert_eq!(functions.len(), context.function_modules.len());
    assert_eq!(functions.len(), context.function_module_names.len());
    assert_eq!(functions.len(), context.function_names.len());

    assert_eq!(context.datatype_names.len(), context.datatype_modules.len());
    assert_eq!(
        context.datatype_names.len(),
        context.datatype_module_names.len()
    );

    assert_eq!(
        context.constructor_names.len(),
        context.constructor_fields.len()
    );
    assert_eq!(
        context.constructor_names.len(),
        context.constructor_modules.len()
    );
    assert_eq!(
        context.constructor_names.len(),
        context.constructor_module_names.len()
    );
    assert_eq!(
        context.constructor_names.len(),
        context.constructor_datatypes.len()
    );
    assert_eq!(
        context.constructor_names.len(),
        context.constructor_datatype_names.len()
    );

    let program = Program { functions };
    (program, context)
}

fn compile(
    cur_module_idx: ModuleIndex,
    _fn_idx: usize,
    instrs: &Vec<bc::RawInstruction>,
    context: &Context,
) -> Vec<Instruction> {
    use bc::RawInstruction;
    instrs
        .iter()
        .map(|instr| match instr {
            RawInstruction::PushInt(i) => Instruction::PushInt(*i),
            RawInstruction::PushString(idx) => {
                // TODO this is incorrect since we merged string table
                //      maybe just get rid of string tables in the bytecode?
                Instruction::PushString(StringTableIndex(*idx))
            }
            RawInstruction::LoadLocal(i) => Instruction::LoadLocal(*i),
            RawInstruction::StoreLocal(i) => Instruction::StoreLocal(*i),
            RawInstruction::Unless(i) => Instruction::Unless(*i),
            RawInstruction::Jump(i) => Instruction::Jump(*i),
            RawInstruction::Call(i) => Instruction::Call(*i),
            RawInstruction::LoadName(ModuleName { module }, fun) => {
                if module.len() == 1 && module[0] == "Prelude" {
                    if !is_intrinsic(fun) {
                        panic!("Prelude::{} doesn't exist", fun)
                    }
                    Instruction::LoadIntrinsic(fun.to_string())
                } else {
                    // TODO resolve module idx first so we can provide better error message
                    let fn_idx = context
                        .function_module_names
                        .iter()
                        .zip(&context.function_names)
                        .position(|(&m_name, &fn_name)| module == m_name && fun == fn_name)
                        .expect("Trying to load a non-existing program name");
                    Instruction::LoadName(FunctionIndex(fn_idx))
                }
            }
            RawInstruction::LoadGlobal(fun) => {
                let fn_idx = context
                    .function_modules
                    .iter()
                    .zip(&context.function_names)
                    .position(|(m_idx, &fn_name)| cur_module_idx == *m_idx && fun == fn_name)
                    .expect("Trying to load a non-existing module name");
                Instruction::LoadName(FunctionIndex(fn_idx))
            }
            RawInstruction::Instantiate(ModuleName { module }, datatype, ctor) => {
                // TODO resolve module idx/datatype idx first so we can provide better error message
                let ctor_idx = context
                    .constructor_module_names
                    .iter()
                    .zip(&context.constructor_datatype_names)
                    .zip(&context.constructor_names)
                    .position(|((&m_name, &dt_name), &ctor_name)| {
                        module == m_name && datatype == dt_name && ctor == ctor_name
                    })
                    .expect("Trying to load a non-existing datatype constructor");
                Instruction::Instantiate(ConstructorIndex(ctor_idx))
            }
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct ModuleIndex(usize);
#[derive(Clone, Copy, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct FunctionIndex(usize);
#[derive(Clone, Copy, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct StringTableIndex(usize);
#[derive(Clone, Copy, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct DatatypeIndex(usize);
#[derive(Clone, Copy, Debug, Ord, PartialOrd, Eq, PartialEq)]
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
    LoadIntrinsic(String),
    Instantiate(ConstructorIndex),
}
