use crate::config::{StrategyARule, StrategyBRule, StrategyCRule};
use std::collections::{hash_map::DefaultHasher, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use syn::visit::{self, Visit};
use syn::{
    Attribute, File, Ident, ImplItem, Item, ItemFn, ItemTrait, TraitItem,
};
use quote::ToTokens;

// --- FnVisitor, TraitVisitor 保持不变 ---

#[derive(Default)]
struct FnVisitor {
    functions: HashMap<Ident, u64>,
}

fn calculate_token_hash<T: ToTokens>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.to_token_stream().to_string().hash(&mut s);
    s.finish()
}

impl<'ast> Visit<'ast> for FnVisitor {
    fn visit_item_fn(&mut self, i: &'ast ItemFn) {
        self.functions.insert(i.sig.ident.clone(), calculate_token_hash(i));
    }
    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        for item in &i.items {
            if let ImplItem::Fn(method) = item {
                let method_hash = calculate_token_hash(method);
                self.functions.insert(method.sig.ident.clone(), method_hash);
            }
        }
        visit::visit_item_impl(self, i);
    }
}

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

// --- 【全新】用于策略B的访问器，只负责提取非测试代码块 ---
#[derive(Default)]
struct CodeBlockVisitor {
    // 存储所有非测试代码块的字符串表示
    non_test_code_blocks: Vec<String>,
    in_test_context: bool,
}

impl<'ast> Visit<'ast> for CodeBlockVisitor {
    // 访问任何带属性的项（函数、模块等）
    fn visit_item(&mut self, i: &'ast Item) {
        let item_attrs = match i {
            Item::Const(c) => &c.attrs, Item::Enum(e) => &e.attrs,
            Item::ExternCrate(e) => &e.attrs, Item::Fn(f) => &f.attrs,
            Item::ForeignMod(f) => &f.attrs, Item::Impl(imp) => &imp.attrs,
            Item::Macro(m) => &m.attrs, Item::Mod(m) => &m.attrs,
            Item::Static(s) => &s.attrs, Item::Struct(s) => &s.attrs,
            Item::Trait(t) => &t.attrs, Item::TraitAlias(t) => &t.attrs,
            Item::Type(t) => &t.attrs, Item::Union(u) => &u.attrs,
            Item::Use(u) => &u.attrs,
            _ => { visit::visit_item(self, i); return; }
        };

        let is_test_item = item_attrs.iter().any(is_test_attribute);

        if is_test_item {
            // 如果是测试项，设置上下文标志并访问子节点，但不记录代码块
            let original_context = self.in_test_context;
            self.in_test_context = true;
            visit::visit_item(self, i);
            self.in_test_context = original_context;
        } else if !self.in_test_context {
            // 如果不是测试项且不在测试上下文中，记录其代码并继续访问
            self.non_test_code_blocks.push(i.to_token_stream().to_string());
            visit::visit_item(self, i);
        } else {
            // 如果在测试上下文中，则仅访问子节点
            visit::visit_item(self, i);
        }
    }
}


fn is_test_attribute(attr: &Attribute) -> bool {
    if let Some(segment) = attr.path().segments.last() {
        let ident_str = segment.ident.to_string();
        if ident_str == "test" || ident_str == "tokio" { return true; }
    }
    if attr.path().is_ident("cfg") {
        if let Ok(list) = attr.meta.require_list() {
            return list.tokens.to_string() == "test";
        }
    }
    false
}

// --- 分析函数 ---

/// 策略 A (无变动)
pub fn analyze_strategy_a(old_ast: &File, new_ast: &File, rule: &StrategyARule) -> Vec<String> {
    let mut reports = Vec::new();
    let mut old_visitor = FnVisitor::default();
    old_visitor.visit_file(old_ast);
    let mut new_visitor = FnVisitor::default();
    new_visitor.visit_file(new_ast);
    for func_name in &rule.functions {
        let func_ident = syn::parse_str::<Ident>(func_name).unwrap();
        match (old_visitor.functions.get(&func_ident), new_visitor.functions.get(&func_ident)) {
            (Some(old_hash), Some(new_hash)) => {
                if old_hash != new_hash {
                    reports.push(format!("[STRATEGY_A] Function or method '{}' in '{}' has been modified.", func_name, rule.file));
                }
            }
            (None, Some(_)) => reports.push(format!("[STRATEGY_A] Function or method '{}' in '{}' has been added.", func_name, rule.file)),
            (Some(_), None) => reports.push(format!("[STRATEGY_A] Function or method '{}' in '{}' has been removed.", func_name, rule.file)),
            (None, None) => {}
        }
    }
    reports
}

/// 【最终修复】策略 B: 结合 AST 上下文过滤和文本搜索
pub fn analyze_strategy_b(file_path: &str, old_ast: &File, new_ast: &File, rule: &StrategyBRule) -> Vec<String> {
    let mut reports = Vec::new();

    if !rule.directories.iter().any(|dir| file_path.starts_with(dir)) {
        return reports;
    }

    // 1. 遍历旧 AST，提取所有非测试代码块
    let mut old_visitor = CodeBlockVisitor::default();
    old_visitor.visit_file(old_ast);
    let old_code_string = old_visitor.non_test_code_blocks.join("\n");

    // 2. 遍历新 AST，提取所有非测试代码块
    let mut new_visitor = CodeBlockVisitor::default();
    new_visitor.visit_file(new_ast);
    let new_code_string = new_visitor.non_test_code_blocks.join("\n");

    // 3. 在过滤后的代码字符串上，进行文本计数比较
    for func_to_watch in &rule.functions {
        let old_count = old_code_string.matches(func_to_watch).count();
        let new_count = new_code_string.matches(func_to_watch).count();

        if new_count > old_count {
            reports.push(format!("[STRATEGY_B] A call to '{}' was added in file '{}' (non-test occurrences changed from {} to {}).", func_to_watch, file_path, old_count, new_count));
        }
        if new_count < old_count {
            reports.push(format!("[STRATEGY_B] A call to '{}' was removed in file '{}' (non-test occurrences changed from {} to {}).", func_to_watch, file_path, old_count, new_count));
        }
    }

    reports
}

/// 策略 C (无变动)
pub fn analyze_strategy_c(old_ast: &File, new_ast: &File, rule: &StrategyCRule) -> Vec<String> {
    let mut reports = Vec::new();
    let mut old_visitor = TraitVisitor::default();
    old_visitor.visit_file(old_ast);
    let mut new_visitor = TraitVisitor::default();
    new_visitor.visit_file(new_ast);
    for trait_name in &rule.traits {
        let trait_ident = syn::parse_str::<Ident>(trait_name).unwrap();
        if let (Some(old_trait), Some(new_trait)) = (old_visitor.traits.get(&trait_ident), new_visitor.traits.get(&trait_ident)) {
            let old_methods: HashSet<Ident> = old_trait.items.iter().filter_map(|item| if let TraitItem::Fn(method) = item { Some(method.sig.ident.clone()) } else { None }).collect();
            let new_methods: HashSet<Ident> = new_trait.items.iter().filter_map(|item| if let TraitItem::Fn(method) = item { Some(method.sig.ident.clone()) } else { None }).collect();
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

