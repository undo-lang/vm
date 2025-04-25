use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde()]
pub struct ModuleName {
    module: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "tag", content = "contents")]
pub enum RawInstruction {
    PushInt(i64),
    PushString(usize),
    LoadLocal(usize),
    StoreLocal(usize),
    LoadName(ModuleName, String),
    LoadGlobal(String),
    Unless(usize),
    Jump(usize),
    Call(usize),
    Instantiate(ModuleName, String, String), // TODO parse to usize
}

#[derive(Serialize, Deserialize)]
pub struct ADTVariant {
    pub name: String,
    pub elements: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ADTDefinition {
    name: String,
    pub variants: Vec<ADTVariant>,
}

#[derive(Serialize, Deserialize)]
pub struct Module {
    pub name: Vec<String>,
    pub strings: Vec<String>,
    pub functions: HashMap<String, Vec<RawInstruction>>,
    pub dependencies: Vec<Vec<String>>,
    pub adts: HashMap<String, ADTDefinition>,
    // TODO expectedADTs
}