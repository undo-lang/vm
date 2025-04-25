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
    // let adts = modules.iter().fold(HashMap::new(), |mut m, (moduleName, module)| {
    //     m.insert(moduleName, module.adts.iter().fold(HashMap::new(), |mut adts, (adtName, def)| {
    //         adts.insert(adtName, def.variants.iter().fold(HashMap::new(), |mut ctors, variant| {
    //             let data = ADTData { id: next, elements: variant.elements.len() };
    //             ctors.insert(&variant.name, data);
    //             next = next + 1;
    //             ctors
    //         }
    //         ));
    //         adts
    //     }));
    //     m
    // });
    let mut adts = HashMap::new();
    for (module_name, module) in modules {
        let mut module_adts = HashMap::new();
        for (adt_name, def) in module.adts.iter() {
            let mut ctors = HashMap::new();
            for variant in def.variants.iter() {
                let data = ADTData { id: next, elements: variant.elements.len() };
                ctors.insert(&variant.name, data);
                next = next + 1;
            }
            module_adts.insert(adt_name, ctors);
        }
        adts.insert(module_name, module_adts);
    }
    Context { adts }
}
