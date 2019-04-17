use std::sync::Arc;
use rustc_hash::FxHashMap;

use ra_syntax::{SmolStr, ast::AttrsOwner};

use crate::{
    Crate, DefDatabase, Enum, Function, HirDatabase, ImplBlock, Module, Static, Struct, Trait
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LangItemTarget {
    Enum(Enum),
    Function(Function),
    ImplBlock(ImplBlock),
    Static(Static),
    Struct(Struct),
    Trait(Trait),
}

impl LangItemTarget {
    pub(crate) fn krate(&self, db: &impl HirDatabase) -> Option<Crate> {
        match self {
            LangItemTarget::Enum(e) => e.module(db).krate(db),
            LangItemTarget::Function(f) => f.module(db).krate(db),
            LangItemTarget::ImplBlock(i) => i.module().krate(db),
            LangItemTarget::Static(s) => s.module(db).krate(db),
            LangItemTarget::Struct(s) => s.module(db).krate(db),
            LangItemTarget::Trait(t) => t.module(db).krate(db),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LangItems {
    items: FxHashMap<SmolStr, LangItemTarget>,
}

impl LangItems {
    pub fn target<'a>(&'a self, item: &str) -> Option<&'a LangItemTarget> {
        self.items.get(item)
    }

    /// Salsa query. This will query a specific crate for lang items.
    pub(crate) fn lang_items_query(db: &impl DefDatabase, krate: Crate) -> Arc<LangItems> {
        let mut lang_items = LangItems { items: FxHashMap::default() };

        if let Some(module) = krate.root_module(db) {
            lang_items.collect_lang_items_recursive(db, &module);
        }

        Arc::new(lang_items)
    }

    fn collect_lang_items_recursive(&mut self, db: &impl DefDatabase, module: &Module) {
        // Look for impl targets
        let (impl_blocks, source_map) = db.impls_in_module_with_source_map(module.clone());
        let source = module.definition_source(db).1;
        for (impl_id, _) in impl_blocks.impls.iter() {
            let impl_block = source_map.get(&source, impl_id);
            let lang_item_name = impl_block
                .attrs()
                .filter_map(|a| a.as_key_value())
                .filter(|(key, _)| key == "lang")
                .map(|(_, val)| val)
                .nth(0);
            if let Some(lang_item_name) = lang_item_name {
                let imp = ImplBlock::from_id(*module, impl_id);
                self.items.entry(lang_item_name).or_insert(LangItemTarget::ImplBlock(imp));
            }
        }

        // FIXME we should look for the other lang item targets (traits, structs, ...)

        // Look for lang items in the children
        for child in module.children(db) {
            self.collect_lang_items_recursive(db, &child);
        }
    }
}

/// Look for a lang item, starting from the specified crate and recursively traversing its
/// dependencies.
pub(crate) fn lang_item_lookup(
    db: &impl DefDatabase,
    start_krate: Crate,
    item: &str,
) -> Option<LangItemTarget> {
    let lang_items = db.lang_items(start_krate);
    let start_krate_target = lang_items.items.get(item);
    if start_krate_target.is_some() {
        start_krate_target.map(|t| *t)
    } else {
        for dep in start_krate.dependencies(db) {
            let dep_krate = dep.krate;
            let dep_target = lang_item_lookup(db, dep_krate, item);
            if dep_target.is_some() {
                return dep_target;
            }
        }
        None
    }
}
