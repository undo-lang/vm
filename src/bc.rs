use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde()]
pub struct ModuleName {
    pub module: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "tag", content = "contents")]
pub enum RawInstruction {
    PushInt(i64),
    PushString(String),
    LoadLocal(usize),
    StoreLocal(usize),
    LoadReg(usize),
    StoreReg(usize),
    LoadName(ModuleName, String),
    LoadGlobal(String),
    Unless(usize),
    Jump(usize),
    Call(usize),
    Instantiate(ModuleName, String, String),
    IsVariant(ModuleName, String, String),
    Field(ModuleName, String, String, String),
}

#[derive(Serialize, Deserialize)]
pub struct ADTVariant {
    pub name: String,
    pub elements: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ExpectedADT {
    pub module: Vec<String>,
    pub name: String,
    pub variants: Vec<ADTVariant>,
}

#[derive(Serialize, Deserialize)]
pub struct Module {
    pub name: Vec<String>,
    pub functions: HashMap<String, Vec<RawInstruction>>,
    pub dependencies: Vec<Vec<String>>,
    pub adts: HashMap<String, Vec<ADTVariant>>,
    pub expected_adts: Vec<ExpectedADT>,
}