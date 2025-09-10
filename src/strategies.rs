// src/strategies.rs
use crate::config::{StrategyARule, StrategyBRule, StrategyCRule};
use std::collections::{HashMap, HashSet};
use syn::visit::{self, Visit};
use syn::{File, Ident, ImplItem, Item, ItemFn, ItemTrait, TraitItem};

#[derive(Default)]
struct FnVisitor {
    // Key: function or method name, Value: 格式化后的函数/方法字符串表示，后续比较两个函数是否“变更”的最简单方法就是比较它们的文本是否完全一样
    functions: HashMap<Ident, String>,
}

// visit::Visit trait 的实现
impl<'ast> Visit<'ast> for FnVisitor {
    // 访问顶层函数 (e.g., `fn my_function() {}`)
    // 寻找functions,存入hashmap
    fn visit_item_fn(&mut self, i: &'ast ItemFn) {
        let fn_str = prettyplease::unparse(&syn::File {
            shebang: None,
            attrs: vec![],
            items: vec![Item::Fn(i.clone())],
        });
        self.functions.insert(i.sig.ident.clone(), fn_str);
    }

    // 访问 impl 块 (e.g., `impl MyStruct { ... }`) 以查找方法
    //遍历 impl 块内部的所有项，找出其中的方法（ImplItem::Fn）
    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        for item in &i.items {
            // 只关心 impl 块中的方法
            if let ImplItem::Fn(method) = item {
                // 使用 prettyplease 将方法节点格式化为字符串
                // 注意：这里只格式化方法本身，而不是整个 impl 块
                let method_item = Item::Fn(ItemFn {
                    attrs: method.attrs.clone(),
                    vis: method.vis.clone(),
                    sig: method.sig.clone(),
                    block: Box::new(method.block.clone()),
                });
                let method_str = prettyplease::unparse(&syn::File {
                    shebang: None,
                    attrs: vec![],
                    items: vec![method_item],
                });
                self.functions.insert(method.sig.ident.clone(), method_str);
            }
        }
        // 继续遍历 impl 块内部的其他项，以处理嵌套的项
        visit::visit_item_impl(self, i);
    }
}


// --- AST Visitor for finding traits (无变动) ---
#[derive(Default)]
struct TraitVisitor<'ast> {
    traits: HashMap<Ident, &'ast ItemTrait>,
}

impl<'ast> Visit<'ast> for TraitVisitor<'ast> {
    fn visit_item_trait(&mut self, i: &'ast ItemTrait) {
        self.traits.insert(i.ident.clone(), i);
        visit::visit_item_trait(self, i);
    }
}

/// 策略 A: 比较指定函数/方法是否有变更
pub fn analyze_strategy_a(old_ast: &File, new_ast: &File, rule: &StrategyARule) -> Vec<String> {
    let mut reports = Vec::new();

    let mut old_visitor = FnVisitor::default();
    old_visitor.visit_file(old_ast);

    let mut new_visitor = FnVisitor::default();
    new_visitor.visit_file(new_ast);

    for func_name in &rule.functions {
        let func_ident = syn::parse_str::<Ident>(func_name).unwrap();

        match (old_visitor.functions.get(&func_ident), new_visitor.functions.get(&func_ident)) {
            (Some(old_fn_str), Some(new_fn_str)) => {
                if old_fn_str != new_fn_str {
                    reports.push(format!("[STRATEGY_A] Function or method '{}' in '{}' has been modified.", func_name, rule.file));
                }
            }
            (None, Some(_)) => {
                reports.push(format!("[STRATEGY_A] Function or method '{}' in '{}' has been added.", func_name, rule.file));
            }
            (Some(_), None) => {
                reports.push(format!("[STRATEGY_A] Function or method '{}' in '{}' has been removed.", func_name, rule.file));
            }
            (None, None) => {}
        }
    }
    reports
}

/// 策略 B: 比较函数/方法名的文本出现次数 (简化版)
pub fn analyze_strategy_b(old_code: &str, new_code: &str, rule: &StrategyBRule) -> Vec<String> {
    let mut reports = Vec::new();
    for func_name in &rule.functions {
        let old_count = old_code.matches(func_name).count();
        let new_count = new_code.matches(func_name).count();

        if new_count > old_count {
            reports.push(format!("[STRATEGY_B] Call points to '{}' may have been added in '{}' (occurrence changed from {} to {}).", func_name, rule.file, old_count, new_count));
        }
        if new_count < old_count {
            reports.push(format!("[STRATEGY_B] Call points to '{}' may have been removed in '{}' (occurrence changed from {} to {}).", func_name, rule.file, old_count, new_count));
        }
    }
    reports
}


/// 策略 C: 比较指定 Trait 的方法列表是否有变更
pub fn analyze_strategy_c(old_ast: &File, new_ast: &File, rule: &StrategyCRule) -> Vec<String> {
    let mut reports = Vec::new();

    let mut old_visitor = TraitVisitor::default();
    old_visitor.visit_file(old_ast);

    let mut new_visitor = TraitVisitor::default();
    new_visitor.visit_file(new_ast);

    for trait_name in &rule.traits {
        let trait_ident = syn::parse_str::<Ident>(trait_name).unwrap();

        if let (Some(old_trait), Some(new_trait)) = (old_visitor.traits.get(&trait_ident), new_visitor.traits.get(&trait_ident)) {
            let old_methods: HashSet<Ident> = old_trait.items.iter().filter_map(|item| match item {
                TraitItem::Fn(method) => Some(method.sig.ident.clone()),
                _ => None
            }).collect();

            let new_methods: HashSet<Ident> = new_trait.items.iter().filter_map(|item| match item {
                TraitItem::Fn(method) => Some(method.sig.ident.clone()),
                _ => None
            }).collect();

            for method in new_methods.difference(&old_methods) {
                reports.push(format!("[STRATEGY_C] Method '{}' was added to trait '{}' in '{}'.", method, trait_name, rule.file));
            }
            for method in old_methods.difference(&new_methods) {
                reports.push(format!("[STRATEGY_C] Method '{}' was removed from trait '{}' in '{}'.", method, trait_name, rule.file));
            }
        }
    }
    reports
}