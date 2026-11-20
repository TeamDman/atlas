//! Calls and explicit bindings from the same AST as the ordered body projection.
use super::{AsyncClosure, Binding, Call, CallbackParameter, Declared, parse::Reader};
use std::collections::{BTreeMap, BTreeSet};
use syn::{Expr, FnArg, Item, Pat, Signature, spanned::Spanned, visit::Visit};

pub(crate) fn imports(
    items: &[Item],
    selection: crate::population::SyntaxSelection<'_>,
) -> BTreeMap<String, Vec<Vec<String>>> {
    bindings(items, selection, false)
}
pub(super) fn exports(
    items: &[Item],
    selection: crate::population::SyntaxSelection<'_>,
) -> BTreeMap<String, Vec<Vec<String>>> {
    bindings(items, selection, true)
}
fn bindings(
    items: &[Item],
    selection: crate::population::SyntaxSelection<'_>,
    public_only: bool,
) -> BTreeMap<String, Vec<Vec<String>>> {
    let mut out = BTreeMap::<String, Vec<Vec<String>>>::new();
    for item in items {
        if let Item::Use(item) = item
            && selection.admits(item.span())
            && (!public_only || matches!(item.vis, syn::Visibility::Public(_)))
        {
            for binding in crate::scan::item_use_bindings(item) {
                // A glob remains an unresolved import; qualification never expands it.
                let path = binding
                    .source_path
                    .split("::")
                    .filter(|part| *part != "*")
                    .map(str::to_owned)
                    .collect();
                out.entry(binding.local_name).or_default().push(path);
            }
        }
    }
    out
}
fn pattern(pat: &Pat) -> Option<String> {
    match pat {
        Pat::Ident(i) => Some(i.ident.to_string()),
        Pat::Type(t) => pattern(&t.pat),
        _ => None,
    }
}

/// Retain uniquely written FnOnce arities on direct type parameters.
pub(super) fn callback_parameters(signature: &Signature) -> Vec<CallbackParameter> {
    fn once_arity(bound: &syn::TypeParamBound) -> Option<usize> {
        let syn::TypeParamBound::Trait(bound) = bound else {
            return None;
        };
        let segment = bound.path.segments.last()?;
        let names: Vec<_> = bound
            .path
            .segments
            .iter()
            .map(|part| part.ident.to_string())
            .collect();
        if !(matches!(names.as_slice(), [name] if name == "FnOnce")
            || matches!(names.as_slice(), [root, module, name] if (root == "std" || root == "core") && module == "ops" && name == "FnOnce"))
        {
            return None;
        }
        let syn::PathArguments::Parenthesized(arguments) = &segment.arguments else {
            return None;
        };
        Some(arguments.inputs.len())
    }
    let mut names = BTreeMap::<String, BTreeSet<usize>>::new();
    for parameter in signature.generics.type_params() {
        for arity in parameter.bounds.iter().filter_map(once_arity) {
            names
                .entry(parameter.ident.to_string())
                .or_default()
                .insert(arity);
        }
    }
    if let Some(where_clause) = &signature.generics.where_clause {
        for predicate in &where_clause.predicates {
            if let syn::WherePredicate::Type(predicate) = predicate
                && let syn::Type::Path(path) = &predicate.bounded_ty
                && path.qself.is_none()
                && let Some(name) = path.path.get_ident()
                && signature
                    .generics
                    .type_params()
                    .any(|parameter| parameter.ident == *name)
            {
                for arity in predicate.bounds.iter().filter_map(once_arity) {
                    names.entry(name.to_string()).or_default().insert(arity);
                }
            }
        }
    }
    signature
        .inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Typed(arg) => Some(arg),
            FnArg::Receiver(_) => None,
        })
        .enumerate()
        .filter_map(|(argument, arg)| {
            let syn::Type::Path(path) = &*arg.ty else {
                return None;
            };
            let name = path.path.get_ident()?.to_string();
            if path.qself.is_some() || !arg.attrs.is_empty() {
                return None;
            }
            let arities = names.get(&name)?;
            if arities.len() != 1 {
                return None;
            }
            Some(CallbackParameter {
                argument,
                name: pattern(&arg.pat)?,
                arity: *arities.first()?,
            })
        })
        .collect()
}

fn async_closures(
    reader: &Reader<'_>,
    args: &syn::punctuated::Punctuated<Expr, syn::Token![,]>,
    callback_names: &BTreeSet<String>,
) -> Vec<AsyncClosure> {
    args.iter()
        .enumerate()
        .filter_map(|(argument, expr)| {
            let Expr::Closure(closure) = expr else {
                return None;
            };
            if !closure.attrs.is_empty()
                || closure.asyncness.is_some()
                || closure.constness.is_some()
            {
                return None;
            }
            let Expr::Async(body) = &*closure.body else {
                return None;
            };
            let parameters: Vec<_> = closure
                .inputs
                .iter()
                .map(|pattern| match pattern {
                    Pat::Ident(ident) if ident.subpat.is_none() => Some(ident.ident.to_string()),
                    Pat::Type(typed) => match &*typed.pat {
                        Pat::Ident(ident) if ident.subpat.is_none() => {
                            Some(ident.ident.to_string())
                        }
                        _ => None,
                    },
                    _ => None,
                })
                .collect::<Option<_>>()?;
            if parameters.iter().collect::<BTreeSet<_>>().len() != parameters.len() {
                return None;
            }
            let mut shadows = ShadowedNames::default();
            shadows.visit_block(&body.block);
            shadows.0.extend(parameters.iter().cloned());
            let mut visitor = Facts {
                reader,
                calls: vec![],
                bindings: vec![],
                items: BTreeSet::new(),
                declared: vec![],
                callback_names: callback_names.difference(&shadows.0).cloned().collect(),
            };
            visitor.visit_block(&body.block);
            Some(AsyncClosure {
                argument,
                parameters,
                site: reader.site(closure.span()),
                body: reader.deferred_block(&body.block),
                awaited_callbacks: visitor
                    .calls
                    .into_iter()
                    .filter(|call| call.awaited_value.is_some())
                    .collect(),
            })
        })
        .collect()
}
pub(super) fn body(
    reader: &Reader<'_>,
    signature: &Signature,
    block: &syn::Block,
) -> (Vec<Call>, Vec<Binding>, Vec<String>, Vec<Declared>) {
    let mut facts = Facts {
        reader,
        calls: vec![],
        bindings: vec![],
        items: BTreeSet::new(),
        declared: vec![],
        callback_names: callback_parameters(signature)
            .into_iter()
            .map(|parameter| parameter.name)
            .collect(),
    };
    for arg in &signature.inputs {
        if let FnArg::Typed(arg) = arg
            && let Some(name) = pattern(&arg.pat)
        {
            facts.bindings.push(Binding {
                name,
                ty: Some(reader.text(arg.ty.span())),
                initializer: None,
                site: reader.site(arg.span()),
            });
        } else if let FnArg::Typed(arg) = arg {
            facts.visit_pat(&arg.pat);
        }
    }
    facts.visit_block(block);
    (
        facts.calls,
        facts.bindings,
        facts.items.into_iter().collect(),
        facts.declared,
    )
}
struct Facts<'a, 'b> {
    reader: &'a Reader<'b>,
    calls: Vec<Call>,
    bindings: Vec<Binding>,
    /// Names bound by items and `use` declarations inside the body.
    items: BTreeSet<String>,
    declared: Vec<Declared>,
    callback_names: BTreeSet<String>,
}
/// The names a body-local item binds; `*` for a glob import. A macro
/// definition binds no call path.
fn item_names(item: &Item) -> Vec<String> {
    let ident = match item {
        Item::Use(item) => {
            return crate::scan::item_use_bindings(item)
                .into_iter()
                .map(|binding| binding.local_name)
                .filter(|name| name != "_")
                .collect();
        }
        Item::ForeignMod(block) => {
            return block
                .items
                .iter()
                .filter_map(|item| match item {
                    syn::ForeignItem::Fn(f) => Some(f.sig.ident.to_string()),
                    syn::ForeignItem::Static(s) => Some(s.ident.to_string()),
                    syn::ForeignItem::Type(t) => Some(t.ident.to_string()),
                    _ => None,
                })
                .collect();
        }
        Item::ExternCrate(item) => item.rename.as_ref().map_or(&item.ident, |(_, name)| name),
        Item::Fn(item) => &item.sig.ident,
        Item::Const(item) => &item.ident,
        Item::Static(item) => &item.ident,
        Item::Struct(item) => &item.ident,
        Item::Enum(item) => &item.ident,
        Item::Union(item) => &item.ident,
        Item::Type(item) => &item.ident,
        Item::Trait(item) => &item.ident,
        Item::TraitAlias(item) => &item.ident,
        Item::Mod(item) => &item.ident,
        _ => return Vec::new(),
    };
    vec![ident.to_string()]
}
fn initializer(expr: &Expr) -> Option<&Expr> {
    match expr {
        Expr::Paren(e) => initializer(&e.expr),
        Expr::Reference(e) => initializer(&e.expr),
        Expr::Call(_) | Expr::MethodCall(_) => Some(expr),
        _ => None,
    }
}
impl<'ast> Visit<'ast> for Facts<'_, '_> {
    fn visit_expr_assign(&mut self, assignment: &'ast syn::ExprAssign) {
        // An overwritten constructor no longer establishes the receiver's type.
        if let Expr::Path(left) = assignment.left.as_ref()
            && let Some(name) = left.path.get_ident()
        {
            self.bindings.push(Binding {
                name: name.to_string(),
                ty: None,
                initializer: None,
                site: self.reader.site(assignment.span()),
            });
        }
        syn::visit::visit_expr_assign(self, assignment);
    }

    fn visit_local(&mut self, local: &'ast syn::Local) {
        if let Some(name) = pattern(&local.pat) {
            let ty = if let Pat::Type(p) = &local.pat {
                Some(self.reader.text(p.ty.span()))
            } else {
                None
            };
            let call = local
                .init
                .as_ref()
                .and_then(|i| initializer(&i.expr))
                .and_then(|e| match e {
                    Expr::Call(c) => Some(self.reader.text(c.func.span())),
                    Expr::MethodCall(_) => {
                        let text = crate::source_text::text(self.reader.source, e.span());
                        (text.len() <= 4096).then_some(text)
                    }
                    _ => None,
                });
            self.bindings.push(Binding {
                name,
                ty,
                initializer: call,
                site: self.reader.site(local.span()),
            });
        } else {
            self.visit_pat(&local.pat);
        }
        if let Some(init) = &local.init {
            self.visit_expr(&init.expr);
            if let Some((_, diverge)) = &init.diverge {
                self.visit_expr(diverge);
            }
        }
    }
    fn visit_pat_ident(&mut self, pat: &'ast syn::PatIdent) {
        self.bindings.push(Binding {
            name: pat.ident.to_string(),
            ty: None,
            initializer: None,
            site: self.reader.site(pat.span()),
        });
        syn::visit::visit_pat_ident(self, pat);
    }
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        self.visit_expr(&call.func);
        for arg in &call.args {
            self.visit_expr(arg);
        }
        self.calls.push(Call {
            spelling: self.reader.text(call.func.span()),
            receiver: None,
            arguments: call
                .args
                .iter()
                .map(|a| self.reader.text(a.span()))
                .collect(),
            site: self.reader.site(call.span()),
            awaited_block_call: false,
            await_producer: None,
            directly_awaited: false,
            awaited_value: None,
            awaited_arity: 0,
            async_closures: async_closures(self.reader, &call.args, &self.callback_names),
        });
    }
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        self.visit_expr(&call.receiver);
        for arg in &call.args {
            self.visit_expr(arg);
        }
        self.calls.push(Call {
            spelling: call.method.to_string(),
            receiver: Some(self.reader.text(call.receiver.span())),
            arguments: call
                .args
                .iter()
                .map(|a| self.reader.text(a.span()))
                .collect(),
            site: self.reader.site(call.span()),
            awaited_block_call: false,
            await_producer: None,
            directly_awaited: false,
            awaited_value: None,
            awaited_arity: 0,
            async_closures: async_closures(self.reader, &call.args, &self.callback_names),
        });
    }
    fn visit_expr_await(&mut self, awaited: &'ast syn::ExprAwait) {
        if let Some(block) = super::parse::direct_async(&awaited.base) {
            let mut shadows = ShadowedNames::default();
            shadows.visit_block(&block.block);
            let mut visitor = AwaitedSelfCalls {
                reader: self.reader,
                calls: &mut self.calls,
                shadows: shadows.0,
            };
            visitor.visit_block(&block.block);
        } else {
            syn::visit::visit_expr_await(self, awaited);
            if let Expr::Call(call) = &*awaited.base {
                let call_site = self.reader.site(call.span());
                if let Some(observation) = self
                    .calls
                    .iter_mut()
                    .find(|observation| observation.site == call_site)
                {
                    observation.directly_awaited = true;
                }
                if let Expr::Path(path) = &*call.func
                    && path.qself.is_none()
                    && let Some(name) = path.path.get_ident()
                    && self.callback_names.contains(&name.to_string())
                {
                    self.calls.push(Call {
                        spelling: name.to_string(),
                        receiver: None,
                        arguments: vec![],
                        site: self.reader.site(awaited.span()),
                        awaited_block_call: false,
                        await_producer: None,
                        directly_awaited: false,
                        awaited_value: Some(name.to_string()),
                        awaited_arity: call.args.len(),
                        async_closures: vec![],
                    });
                }
            }
            if let Expr::MethodCall(call) = &*awaited.base {
                let call_site = self.reader.site(call.span());
                if let Some(observation) = self
                    .calls
                    .iter_mut()
                    .find(|observation| observation.site == call_site)
                {
                    observation.directly_awaited = true;
                }
            }
            if let Expr::MethodCall(call) = &*awaited.base
                && matches!(&*call.receiver, Expr::Struct(_))
            {
                self.calls.push(Call {
                    spelling: call.method.to_string(),
                    receiver: Some(self.reader.text(call.receiver.span())),
                    arguments: call
                        .args
                        .iter()
                        .map(|a| self.reader.text(a.span()))
                        .collect(),
                    site: self.reader.site(awaited.span()),
                    awaited_block_call: false,
                    await_producer: Some(self.reader.site(call.span())),
                    directly_awaited: false,
                    awaited_value: None,
                    awaited_arity: 0,
                    async_closures: vec![],
                });
            }
        }
    }
    fn visit_expr_closure(&mut self, _: &'ast syn::ExprClosure) {}
    fn visit_expr_async(&mut self, _: &'ast syn::ExprAsync) {}
    fn visit_expr_macro(&mut self, _: &'ast syn::ExprMacro) {}
    fn visit_expr_const(&mut self, _: &'ast syn::ExprConst) {}
    /// A nested item's body is its own scope, but its name shadows the
    /// module's throughout the enclosing block.
    fn visit_item(&mut self, item: &'ast Item) {
        self.items.extend(item_names(item));
        // A function is named by its signature, not its whole body.
        let header = match item {
            Item::Fn(f) => f.sig.span(),
            _ => item.span(),
        };
        self.declared.push(Declared {
            label: self.reader.text(header),
            site: self.reader.site(item.span()),
        });
    }
    fn visit_arm(&mut self, arm: &'ast syn::Arm) {
        self.visit_pat(&arm.pat);
        self.visit_expr(&arm.body);
    }
}

/// Conservatively refuse a path if any binding anywhere in the block may
/// shadow it. This can omit safe calls, but never assumes local scope wins.
#[derive(Default)]
struct ShadowedNames(BTreeSet<String>);
impl<'ast> Visit<'ast> for ShadowedNames {
    fn visit_expr_assign(&mut self, assignment: &'ast syn::ExprAssign) {
        if let Expr::Path(path) = assignment.left.as_ref()
            && let Some(name) = path.path.get_ident()
        {
            self.0.insert(name.to_string());
        }
        syn::visit::visit_expr_assign(self, assignment);
    }
    fn visit_pat_ident(&mut self, pat: &'ast syn::PatIdent) {
        self.0.insert(pat.ident.to_string());
        syn::visit::visit_pat_ident(self, pat);
    }
    fn visit_item(&mut self, item: &'ast syn::Item) {
        self.0.extend(item_names(item));
    }
}

/// Direct awaits are anchored at their await site, not at future construction.
struct AwaitedSelfCalls<'a, 'b, 'c> {
    reader: &'a Reader<'b>,
    calls: &'c mut Vec<Call>,
    shadows: BTreeSet<String>,
}
impl<'ast> Visit<'ast> for AwaitedSelfCalls<'_, '_, '_> {
    fn visit_expr_await(&mut self, awaited: &'ast syn::ExprAwait) {
        if let Some(block) = super::parse::direct_async(&awaited.base) {
            self.visit_block(&block.block);
        } else {
            syn::visit::visit_expr_await(self, awaited);
            if let Some(call) = direct_self_call(&awaited.base) {
                self.calls.push(Call {
                    spelling: call.method.to_string(),
                    receiver: Some("self".into()),
                    arguments: call
                        .args
                        .iter()
                        .map(|a| self.reader.text(a.span()))
                        .collect(),
                    site: self.reader.site(awaited.span()),
                    awaited_block_call: true,
                    await_producer: None,
                    directly_awaited: false,
                    awaited_value: None,
                    awaited_arity: 0,
                    async_closures: vec![],
                });
            } else if let Some(call) = direct_path_call(&awaited.base)
                && let Expr::Path(path) = &*call.func
                && let Some(first) = path.path.segments.first()
                && !self.shadows.contains(&first.ident.to_string())
                && (!self.shadows.contains("*")
                    || matches!(first.ident.to_string().as_str(), "crate" | "self" | "super"))
            {
                self.calls.push(Call {
                    spelling: self.reader.text(call.func.span()),
                    receiver: None,
                    arguments: call
                        .args
                        .iter()
                        .map(|a| self.reader.text(a.span()))
                        .collect(),
                    site: self.reader.site(awaited.span()),
                    awaited_block_call: true,
                    await_producer: None,
                    directly_awaited: false,
                    awaited_value: None,
                    awaited_arity: 0,
                    async_closures: vec![],
                });
            }
        }
    }
    fn visit_expr_async(&mut self, _: &'ast syn::ExprAsync) {}
    fn visit_expr_closure(&mut self, _: &'ast syn::ExprClosure) {}
    fn visit_expr_macro(&mut self, _: &'ast syn::ExprMacro) {}
    fn visit_expr_const(&mut self, _: &'ast syn::ExprConst) {}
    fn visit_item(&mut self, _: &'ast syn::Item) {}
}

fn direct_path_call(expr: &Expr) -> Option<&syn::ExprCall> {
    match expr {
        Expr::Call(call) if matches!(&*call.func, Expr::Path(path) if path.qself.is_none()) => {
            Some(call)
        }
        Expr::Paren(inner) => direct_path_call(&inner.expr),
        Expr::Group(inner) => direct_path_call(&inner.expr),
        _ => None,
    }
}

fn direct_self_call(expr: &Expr) -> Option<&syn::ExprMethodCall> {
    match expr {
        Expr::MethodCall(call) if matches!(&*call.receiver, Expr::Path(p) if p.qself.is_none() && p.path.is_ident("self")) => {
            Some(call)
        }
        Expr::Paren(inner) => direct_self_call(&inner.expr),
        Expr::Group(inner) => direct_self_call(&inner.expr),
        _ => None,
    }
}
