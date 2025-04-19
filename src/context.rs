use crate::vm::Module;
use std::collections::HashMap;

pub struct ADTData {
    pub id: usize,
    pub elements: usize
}

pub struct Context<'a> {
    pub adts: HashMap<&'a Vec<String>, HashMap<&'a String, HashMap<&'a String, ADTData>>>,
}

pub fn build_context(modules: &HashMap<Vec<String>, Module>) -> Context {
    let mut next: usize = 0;
    let mut adts = HashMap::new();
    for (moduleName, module) in modules {
        let mut module_adts = HashMap::new();
        for (adtName, def) in module.adts.iter() {
            let mut ctors = HashMap::new();
            for variant in def.variants.iter() {
                let data = ADTData { id: next, elements: variant.elements.len() };
                ctors.insert(&variant.name, data);
                next = next + 1;
            }
            module_adts.insert(adtName, ctors);
        }
        adts.insert(moduleName, module_adts);
    }
    Context { adts }
}
