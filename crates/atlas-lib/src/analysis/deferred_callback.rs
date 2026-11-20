//! Conservative source witnesses for callbacks taken and called inside a
//! written `Future::poll`; this is not callback resolution or a runtime trace.
use crate::{Result, analysis::workflow::Site};
use serde::Serialize;
use syn::{Expr, ImplItem, Item, Member, Pat, Stmt, spanned::Spanned, visit::Visit};

/// A source-only possibility: a written poll body takes a field, then may call
/// its binding after a written guard's returning branch has been bypassed.
/// Whether this is the standard `Future` trait, the future is polled, the
/// pattern matches, or the callback is reached is not proved.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct DeferredCallbackWitness {
    pub owner: String,
    pub field: String,
    pub binding: String,
    pub guard: String,
    pub poll_site: Site,
    pub take_site: Site,
    pub guard_site: Site,
    pub return_site: Site,
    pub invocation_site: Site,
    /// This observation does not show that construction polls or invokes work.
    pub requires_poll_and_guard_fallthrough: bool,
}

/// A written callback parameter passed to a matching constructor which stores
/// it in the observed field. Names, imports, cfg, return flow and actual polling
/// are not compiler-resolved; this is not an invocation edge.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct ForwardedCallbackWitness {
    pub wrapper_owner: String,
    pub wrapper_method: String,
    pub callback_parameter: String,
    /// Argument index excluding any method receiver.
    pub callback_argument_index: usize,
    pub constructor: String,
    pub constructor_parameter: String,
    pub constructor_site: Site,
    pub stored_field_site: Site,
    pub call_site: Site,
    pub poll: DeferredCallbackWitness,
    pub requires_poll_and_guard_fallthrough: bool,
}

/// Correlate written names in two source files. This never opens a callback body
/// or asserts that a constructor result is returned, polled or admitted.
pub fn inspect_forwarders(
    future_source: &str,
    future_path: &str,
    wrapper_source: &str,
    wrapper_path: &str,
) -> Result<Vec<ForwardedCallbackWitness>> {
    let poll_witnesses = inspect(future_source, future_path)?;
    let future = crate::source::parse_file(future_source)?;
    let wrapper = crate::source::parse_file(wrapper_source)?;
    let mut out = Vec::new();
    for poll in poll_witnesses {
        let syn::Type::Path(owner) = syn::parse_str::<syn::Type>(&poll.owner)? else {
            continue;
        };
        let Some(owner_name) = owner.path.segments.last().map(|s| s.ident.to_string()) else {
            continue;
        };
        let constructors = constructors(&future, &owner_name, &poll.field, future_path);
        let [constructor] = constructors.as_slice() else {
            continue;
        };
        for item in &wrapper.items {
            let Item::Impl(implementation) = item else {
                continue;
            };
            if implementation.trait_.is_some() {
                continue;
            }
            for member in &implementation.items {
                let ImplItem::Fn(method) = member else {
                    continue;
                };
                let mut finder = ForwarderFinder {
                    owner: &owner_name,
                    constructor: &constructor.name,
                    parameter_index: constructor.parameter_index,
                    method,
                    sites: Vec::new(),
                };
                finder.visit_block(&method.block);
                for (site, parameter) in finder.sites {
                    let Some(callback_argument_index) =
                        method_argument_index(&method.sig, &parameter)
                    else {
                        continue;
                    };
                    out.push(ForwardedCallbackWitness {
                        wrapper_owner: crate::source_text::text(
                            wrapper_source,
                            implementation.self_ty.span(),
                        ),
                        wrapper_method: method.sig.ident.to_string(),
                        callback_parameter: parameter,
                        callback_argument_index,
                        constructor: constructor.name.clone(),
                        constructor_parameter: constructor.parameter.clone(),
                        constructor_site: constructor.site.clone(),
                        stored_field_site: constructor.field_site.clone(),
                        call_site: Site::at(wrapper_path, site),
                        poll: poll.clone(),
                        requires_poll_and_guard_fallthrough: true,
                    });
                }
            }
        }
    }
    Ok(out)
}

/// A source-only correlation from a method's typed `self` field through a
/// written forwarding closure to one of the supplied dependency witnesses.
/// No edge to the closure body is asserted: returning, polling, admission,
/// name resolution and macro expansion remain unproved.
#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct TargetForwarderWitness {
    pub owner: String,
    pub method: String,
    pub field: String,
    pub callback_parameter: String,
    pub field_site: Site,
    pub forwarding_call_site: Site,
    pub forwarded_callback_site: Site,
    pub dependency: ForwardedCallbackWitness,
    pub requires_poll_and_guard_fallthrough: bool,
}

/// Correlate only explicit single-name imports from `dependency_crate`, a
/// uniquely written `self` field, and a direct closure call of a method's
/// `FnOnce` parameter. This is a source candidate, not compiler resolution.
pub fn inspect_target_forwarders(
    target_source: &str,
    target_path: &str,
    dependency_crate: &str,
    dependencies: &[ForwardedCallbackWitness],
) -> Result<Vec<TargetForwarderWitness>> {
    let ast = crate::source::parse_file(target_source)?;
    let imports =
        super::workflow::facts::imports(&ast.items, crate::population::SyntaxSelection::All);
    let mut out = Vec::new();
    for item in &ast.items {
        let Item::Impl(implementation) = item else {
            continue;
        };
        if implementation.trait_.is_some() {
            continue;
        }
        let syn::Type::Path(owner) = &*implementation.self_ty else {
            continue;
        };
        let Some(owner_name) = owner.path.get_ident() else {
            continue;
        };
        let structs: Vec<_> = ast
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Struct(s) if &s.ident == owner_name && s.generics.params.is_empty() => {
                    Some(s)
                }
                _ => None,
            })
            .collect();
        let [structure] = structs.as_slice() else {
            continue;
        };
        let owner_name = owner_name.to_string();
        for member in &implementation.items {
            let ImplItem::Fn(method) = member else {
                continue;
            };
            let mut finder = TargetForwarderFinder {
                path: target_path,
                imports: &imports,
                dependency_crate,
                dependencies,
                structure,
                method,
                owner_name: &owner_name,
                results: Vec::new(),
            };
            finder.visit_block(&method.block);
            out.extend(finder.results);
        }
    }
    Ok(out)
}

struct TargetForwarderFinder<'a> {
    path: &'a str,
    imports: &'a std::collections::BTreeMap<String, Vec<Vec<String>>>,
    dependency_crate: &'a str,
    dependencies: &'a [ForwardedCallbackWitness],
    structure: &'a syn::ItemStruct,
    method: &'a syn::ImplItemFn,
    owner_name: &'a str,
    results: Vec<TargetForwarderWitness>,
}
impl<'ast> Visit<'ast> for TargetForwarderFinder<'_> {
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        let Expr::Field(field) = &*call.receiver else {
            syn::visit::visit_expr_method_call(self, call);
            return;
        };
        let Member::Named(field_name) = &field.member else {
            return;
        };
        if !matches!(&*field.base, Expr::Path(path) if path.qself.is_none() && path.path.is_ident("self"))
        {
            return;
        }
        let fields: Vec<_> = self
            .structure
            .fields
            .iter()
            .filter(|field| field.ident.as_ref() == Some(field_name))
            .collect();
        let [declared] = fields.as_slice() else {
            return;
        };
        let syn::Type::Path(field_type) = &declared.ty else {
            return;
        };
        let Some(local_type) = field_type.path.get_ident().map(ToString::to_string) else {
            return;
        };
        let Some([import]) = self.imports.get(&local_type).map(Vec::as_slice) else {
            return;
        };
        for dependency in self.dependencies {
            if call.method != dependency.wrapper_method
                || import.as_slice() != [self.dependency_crate, dependency.wrapper_owner.as_str()]
            {
                continue;
            }
            let Some(Expr::Closure(closure)) =
                call.args.iter().nth(dependency.callback_argument_index)
            else {
                continue;
            };
            let Expr::Call(forwarded) = &*closure.body else {
                continue;
            };
            let Expr::Path(parameter) = &*forwarded.func else {
                continue;
            };
            let Some(parameter) = parameter.path.get_ident().map(ToString::to_string) else {
                continue;
            };
            if !callback_parameter(&self.method.sig, &parameter)
                || renamed(&self.method.block, &parameter)
            {
                continue;
            }
            self.results.push(TargetForwarderWitness {
                owner: self.owner_name.to_owned(),
                method: self.method.sig.ident.to_string(),
                field: field_name.to_string(),
                callback_parameter: parameter,
                field_site: Site::at(self.path, declared.span()),
                forwarding_call_site: Site::at(self.path, call.span()),
                forwarded_callback_site: Site::at(self.path, forwarded.span()),
                dependency: dependency.clone(),
                requires_poll_and_guard_fallthrough: true,
            });
        }
        syn::visit::visit_expr_method_call(self, call);
    }
    fn visit_expr_async(&mut self, _: &'ast syn::ExprAsync) {}
    fn visit_expr_closure(&mut self, _: &'ast syn::ExprClosure) {}
    fn visit_expr_macro(&mut self, _: &'ast syn::ExprMacro) {}
    fn visit_item(&mut self, _: &'ast Item) {}
}

fn method_argument_index(signature: &syn::Signature, name: &str) -> Option<usize> {
    signature
        .inputs
        .iter()
        .filter_map(|input| match input {
            syn::FnArg::Typed(arg) => Some(arg),
            syn::FnArg::Receiver(_) => None,
        })
        .position(|arg| matches!(&*arg.pat, Pat::Ident(pat) if pat.ident == name))
}

struct Constructor {
    name: String,
    parameter: String,
    parameter_index: usize,
    site: Site,
    field_site: Site,
}
fn constructors(ast: &syn::File, owner: &str, field: &str, path: &str) -> Vec<Constructor> {
    let mut out = Vec::new();
    for item in &ast.items {
        let Item::Impl(implementation) = item else {
            continue;
        };
        if implementation.trait_.is_some() {
            continue;
        }
        let syn::Type::Path(ty) = &*implementation.self_ty else {
            continue;
        };
        if ty.path.segments.last().is_none_or(|s| s.ident != owner) {
            continue;
        }
        for member in &implementation.items {
            let ImplItem::Fn(method) = member else {
                continue;
            };
            if method.sig.receiver().is_some() {
                continue;
            }
            let Some(Stmt::Expr(Expr::Struct(value), _)) = method.block.stmts.last() else {
                continue;
            };
            if !value.path.is_ident("Self") {
                continue;
            }
            for assigned in &value.fields {
                if !matches!(&assigned.member, Member::Named(name) if name == field) {
                    continue;
                }
                let Some(parameter) = stored_parameter(&assigned.expr) else {
                    continue;
                };
                if renamed(&method.block, &parameter) {
                    continue;
                }
                let matching: Vec<_> = method.sig.inputs.iter().enumerate().filter_map(|(index, arg)| match arg {
                    syn::FnArg::Typed(arg) if matches!(&*arg.pat, Pat::Ident(name) if name.ident == parameter) => Some(index),
                    _ => None,
                }).collect();
                let [parameter_index] = matching.as_slice() else {
                    continue;
                };
                out.push(Constructor {
                    name: method.sig.ident.to_string(),
                    parameter,
                    parameter_index: *parameter_index,
                    site: Site::at(path, method.sig.span()),
                    field_site: Site::at(path, assigned.span()),
                });
            }
        }
    }
    out
}
fn stored_parameter(expr: &Expr) -> Option<String> {
    let Expr::Call(some) = expr else { return None };
    if !matches!(&*some.func, Expr::Path(path) if path.path.is_ident("Some"))
        || some.args.len() != 1
    {
        return None;
    }
    let Expr::Call(boxed) = some.args.first()? else {
        return None;
    };
    let Expr::Path(path) = &*boxed.func else {
        return None;
    };
    if path
        .path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        != ["Box", "new"]
        || boxed.args.len() != 1
    {
        return None;
    }
    let Expr::Path(parameter) = boxed.args.first()? else {
        return None;
    };
    Some(parameter.path.get_ident()?.to_string())
}

struct ForwarderFinder<'a> {
    owner: &'a str,
    constructor: &'a str,
    parameter_index: usize,
    method: &'a syn::ImplItemFn,
    sites: Vec<(proc_macro2::Span, String)>,
}
impl<'ast> Visit<'ast> for ForwarderFinder<'_> {
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let Expr::Path(path) = &*call.func {
            let names: Vec<_> = path
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            if names.ends_with(&[self.owner.to_owned(), self.constructor.to_owned()])
                && let Some(Expr::Path(argument)) = call.args.iter().nth(self.parameter_index)
                && let Some(parameter) = argument.path.get_ident()
                && callback_parameter(&self.method.sig, &parameter.to_string())
                && !renamed(&self.method.block, &parameter.to_string())
            {
                self.sites.push((call.span(), parameter.to_string()));
            }
        }
        syn::visit::visit_expr_call(self, call);
    }
    fn visit_expr_async(&mut self, _: &'ast syn::ExprAsync) {}
    fn visit_expr_closure(&mut self, _: &'ast syn::ExprClosure) {}
    fn visit_expr_macro(&mut self, _: &'ast syn::ExprMacro) {}
    fn visit_item(&mut self, _: &'ast Item) {}
}
fn callback_parameter(signature: &syn::Signature, name: &str) -> bool {
    let parameter_type = signature.inputs.iter().find_map(|arg| match arg {
        syn::FnArg::Typed(arg) if matches!(&*arg.pat, Pat::Ident(pat) if pat.ident == name) => {
            match &*arg.ty {
                syn::Type::Path(path) if path.qself.is_none() => {
                    path.path.get_ident().map(ToString::to_string)
                }
                _ => None,
            }
        }
        _ => None,
    });
    let Some(parameter_type) = parameter_type else {
        return false;
    };
    let has_fn_once = |bounds: &syn::punctuated::Punctuated<
        syn::TypeParamBound,
        syn::Token![+],
    >| {
        bounds.iter().any(|bound| matches!(bound, syn::TypeParamBound::Trait(trait_bound) if trait_bound.path.segments.last().is_some_and(|s| s.ident == "FnOnce")))
    };
    signature.generics.type_params().any(|param| param.ident == parameter_type && has_fn_once(&param.bounds))
        || signature.generics.where_clause.as_ref().is_some_and(|clause| clause.predicates.iter().any(|predicate| matches!(predicate, syn::WherePredicate::Type(predicate) if matches!(&predicate.bounded_ty, syn::Type::Path(path) if path.qself.is_none() && path.path.is_ident(&parameter_type)) && has_fn_once(&predicate.bounds))))
}

/// Read one source file without evaluating cfg, expanding macros or running it.
/// Only directly written poll bodies and simple `Some(binding) = value.field.take()`
/// shapes are considered. `path` labels provenance and is never opened.
pub fn inspect(source: &str, path: &str) -> Result<Vec<DeferredCallbackWitness>> {
    let ast = crate::source::parse_file(source)?;
    let mut witnesses = Vec::new();
    for item in &ast.items {
        let Item::Impl(implementation) = item else {
            continue;
        };
        let Some((trait_path, _)) = &implementation.trait_ else {
            continue;
        };
        if trait_path
            .segments
            .last()
            .is_none_or(|segment| segment.ident != "Future")
        {
            continue;
        }
        let syn::Type::Path(owner) = &*implementation.self_ty else {
            continue;
        };
        if owner.qself.is_some() {
            continue;
        }
        let Some(owner_name) = owner.path.segments.last().map(|s| s.ident.to_string()) else {
            continue;
        };
        let structs: Vec<_> = ast
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Struct(s) if s.ident == owner_name => Some(s),
                _ => None,
            })
            .collect();
        let [structure] = structs.as_slice() else {
            continue;
        };
        let polls: Vec<_> = implementation
            .items
            .iter()
            .filter_map(|item| match item {
                ImplItem::Fn(f) if f.sig.ident == "poll" => Some(f),
                _ => None,
            })
            .collect();
        let [poll] = polls.as_slice() else { continue };
        for stmt in &poll.block.stmts {
            let Stmt::Expr(Expr::If(branch), _) = stmt else {
                continue;
            };
            let Some((binding, base, field, take_site)) = taken_binding(&branch.cond, path) else {
                continue;
            };
            if !structure
                .fields
                .iter()
                .any(|f| f.ident.as_ref().is_some_and(|name| name == &field))
            {
                continue;
            }
            if base != "self" && !self_alias_before(&poll.block.stmts, &base, branch.span()) {
                continue;
            }
            if renamed(&branch.then_branch, &binding) {
                continue;
            }
            for (index, statement) in branch.then_branch.stmts.iter().enumerate() {
                let Stmt::Expr(Expr::If(guard), _) = statement else {
                    continue;
                };
                let Some(Stmt::Expr(Expr::Return(ret), _)) = guard.then_branch.stmts.last() else {
                    continue;
                };
                if guard
                    .attrs
                    .iter()
                    .chain(&ret.attrs)
                    .any(|a| a.path().is_ident("cfg") || a.path().is_ident("cfg_attr"))
                {
                    continue;
                }
                if calls(guard.cond.as_ref(), &binding).is_some()
                    || guard
                        .then_branch
                        .stmts
                        .iter()
                        .any(|s| calls(s, &binding).is_some())
                {
                    continue;
                }
                let Some(invocation_site) = branch.then_branch.stmts[index + 1..]
                    .iter()
                    .find_map(|s| calls(s, &binding))
                else {
                    continue;
                };
                witnesses.push(DeferredCallbackWitness {
                    owner: crate::source_text::text(source, owner.span()),
                    field: field.clone(),
                    binding: binding.clone(),
                    guard: crate::source_text::text(source, guard.cond.span()),
                    poll_site: Site::at(path, poll.sig.span()),
                    take_site: take_site.clone(),
                    guard_site: Site::at(path, guard.span()),
                    return_site: Site::at(path, ret.span()),
                    invocation_site: Site::at(path, invocation_site),
                    requires_poll_and_guard_fallthrough: true,
                });
            }
        }
    }
    Ok(witnesses)
}

fn taken_binding(expr: &Expr, path: &str) -> Option<(String, String, String, Site)> {
    let Expr::Let(let_expr) = expr else {
        return None;
    };
    let Pat::TupleStruct(pattern) = &*let_expr.pat else {
        return None;
    };
    if !pattern.path.is_ident("Some") {
        return None;
    }
    let mut elements = pattern.elems.iter();
    let Pat::Ident(binding) = elements.next()? else {
        return None;
    };
    if elements.next().is_some() {
        return None;
    }
    if binding.by_ref.is_some() || binding.subpat.is_some() {
        return None;
    }
    let Expr::MethodCall(take) = &*let_expr.expr else {
        return None;
    };
    if take.method != "take" || !take.args.is_empty() {
        return None;
    }
    let Expr::Field(field) = &*take.receiver else {
        return None;
    };
    let Member::Named(name) = &field.member else {
        return None;
    };
    let Expr::Path(base) = &*field.base else {
        return None;
    };
    let base = base.path.get_ident()?.to_string();
    Some((
        binding.ident.to_string(),
        base,
        name.to_string(),
        Site::at(path, take.span()),
    ))
}

fn self_alias_before(statements: &[Stmt], alias: &str, before: proc_macro2::Span) -> bool {
    let mut found = 0;
    for statement in statements {
        let start = statement.span().start();
        let limit = before.start();
        if (start.line, start.column) >= (limit.line, limit.column) {
            break;
        }
        let Stmt::Local(local) = statement else {
            let mut finder = CallFinder {
                name: alias,
                ..CallFinder::default()
            };
            finder.visit_stmt(statement);
            if finder.shadowed {
                return false;
            }
            continue;
        };
        let Pat::Ident(name) = &local.pat else {
            let mut finder = CallFinder {
                name: alias,
                ..CallFinder::default()
            };
            finder.visit_stmt(statement);
            if finder.shadowed {
                return false;
            }
            continue;
        };
        if name.ident != alias {
            let mut finder = CallFinder {
                name: alias,
                ..CallFinder::default()
            };
            finder.visit_stmt(statement);
            if finder.shadowed {
                return false;
            }
            continue;
        }
        let Some(init) = &local.init else {
            return false;
        };
        let Expr::MethodCall(call) = &*init.expr else {
            return false;
        };
        if call.method != "get_mut"
            || !call.args.is_empty()
            || !matches!(&*call.receiver, Expr::Path(p) if p.path.is_ident("self"))
        {
            return false;
        }
        found += 1;
    }
    found == 1
}

#[derive(Default)]
struct CallFinder<'a> {
    name: &'a str,
    found: Option<proc_macro2::Span>,
    shadowed: bool,
}
impl<'ast> Visit<'ast> for CallFinder<'_> {
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if matches!(&*call.func, Expr::Path(p) if p.qself.is_none() && p.path.is_ident(self.name)) {
            self.found.get_or_insert(call.span());
        }
        syn::visit::visit_expr_call(self, call);
    }
    fn visit_pat_ident(&mut self, pat: &'ast syn::PatIdent) {
        if pat.ident == self.name {
            self.shadowed = true;
        }
        syn::visit::visit_pat_ident(self, pat);
    }
    fn visit_expr_assign(&mut self, expr: &'ast syn::ExprAssign) {
        if matches!(&*expr.left, Expr::Path(p) if p.path.is_ident(self.name)) {
            self.shadowed = true;
        }
        syn::visit::visit_expr_assign(self, expr);
    }
    fn visit_expr_async(&mut self, _: &'ast syn::ExprAsync) {}
    fn visit_expr_closure(&mut self, _: &'ast syn::ExprClosure) {}
    fn visit_expr_macro(&mut self, _: &'ast syn::ExprMacro) {}
    fn visit_item(&mut self, _: &'ast Item) {}
}

fn calls<'ast, T: VisitTarget<'ast>>(target: &'ast T, name: &str) -> Option<proc_macro2::Span> {
    let mut finder = CallFinder {
        name,
        ..CallFinder::default()
    };
    target.visit(&mut finder);
    (!finder.shadowed).then_some(finder.found).flatten()
}
fn renamed(block: &syn::Block, name: &str) -> bool {
    let mut finder = CallFinder {
        name,
        ..CallFinder::default()
    };
    finder.visit_block(block);
    finder.shadowed
}
trait VisitTarget<'ast> {
    fn visit(&'ast self, finder: &mut CallFinder<'_>);
}
impl<'ast> VisitTarget<'ast> for Expr {
    fn visit(&'ast self, finder: &mut CallFinder<'_>) {
        finder.visit_expr(self);
    }
}
impl<'ast> VisitTarget<'ast> for Stmt {
    fn visit(&'ast self, finder: &mut CallFinder<'_>) {
        finder.visit_stmt(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guarded_fixture_runs_only_when_polled_and_admitted() {
        use std::{
            cell::Cell,
            future::Future,
            pin::Pin,
            rc::Rc,
            task::{Context, Poll, Waker},
        };

        struct Deferred {
            callback: Option<Box<dyn FnOnce()>>,
            cancelled: bool,
        }
        impl Future for Deferred {
            type Output = ();
            fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> {
                let this = self.get_mut();
                if let Some(callback) = this.callback.take() {
                    if this.cancelled {
                        return Poll::Ready(());
                    }
                    callback();
                }
                Poll::Ready(())
            }
        }
        let calls = Rc::new(Cell::new(0));
        let callback = || {
            let calls = Rc::clone(&calls);
            Box::new(move || calls.set(calls.get() + 1)) as Box<dyn FnOnce()>
        };
        drop(Deferred {
            callback: Some(callback()),
            cancelled: false,
        });
        assert_eq!(
            calls.get(),
            0,
            "constructing without polling does not invoke"
        );
        drop(callback());
        assert_eq!(calls.get(), 0, "ignoring the callback does not invoke");
        let mut context = Context::from_waker(Waker::noop());
        let mut cancelled = Deferred {
            callback: Some(callback()),
            cancelled: true,
        };
        assert!(Pin::new(&mut cancelled).poll(&mut context).is_ready());
        assert_eq!(calls.get(), 0, "cancellation returns before invocation");
        let mut admitted = Deferred {
            callback: Some(callback()),
            cancelled: false,
        };
        assert!(Pin::new(&mut admitted).poll(&mut context).is_ready());
        assert_eq!(calls.get(), 1);
    }

    fn sample(tail: &str) -> String {
        format!(
            "struct Deferred<C> {{ callback: Option<C> }} impl<C> Future for Deferred<C> {{ fn poll(self: Pin<&mut Self>) -> Poll<()> {{ let this = self.get_mut(); if let Some(f) = this.callback.take() {{ if cancelled() {{ return Poll::Ready(()); }} {tail} }} Poll::Pending }} }}"
        )
    }
    #[test]
    fn guarded_callback_is_only_a_poll_conditional_source_witness() {
        let findings = inspect(&sample("f();"), "fixture.rs").unwrap();
        assert_eq!(findings.len(), 1);
        let finding = &findings[0];
        assert!(finding.requires_poll_and_guard_fallthrough);
        assert_eq!(finding.field, "callback");
        assert_eq!(finding.binding, "f");
        assert_eq!(finding.guard, "cancelled()");
        assert!(finding.return_site.line <= finding.invocation_site.line);
    }
    #[test]
    fn construction_without_poll_and_ignored_callback_do_not_create_witnesses() {
        assert!(inspect("struct Deferred<C>{ callback: Option<C> } impl<C> Deferred<C> { fn new(callback:C)->Self{Self{callback:Some(callback)}} }", "fixture.rs").unwrap().is_empty());
        assert!(
            inspect(&sample("drop(f);"), "fixture.rs")
                .unwrap()
                .is_empty()
        );
        assert!(
            inspect(&sample("let later = || f();"), "fixture.rs")
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn rejection_must_return_before_call_and_binding_must_not_be_replaced() {
        assert!(
            inspect(&sample("let f = other; f();"), "fixture.rs")
                .unwrap()
                .is_empty()
        );
        let called_before_return = sample("f();").replace(
            "if cancelled() { return Poll::Ready(()); }",
            "if cancelled() { f(); return Poll::Ready(()); }",
        );
        assert!(
            inspect(&called_before_return, "fixture.rs")
                .unwrap()
                .is_empty()
        );
        let conditional_return = sample("f();").replace(
            "if cancelled() { return Poll::Ready(()); }",
            "if cancelled() { if maybe() { return Poll::Ready(()); } }",
        );
        assert!(
            inspect(&conditional_return, "fixture.rs")
                .unwrap()
                .is_empty()
        );
        let reassigned = sample("f();").replace(
            "let this = self.get_mut();",
            "let mut this = self.get_mut(); this = other;",
        );
        assert!(inspect(&reassigned, "fixture.rs").unwrap().is_empty());
        let conditional_guard = sample("f();").replace(
            "if cancelled()",
            "#[cfg(feature = \"cancel\")] if cancelled()",
        );
        assert!(
            inspect(&conditional_guard, "fixture.rs")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn constructor_forwarders_are_witnesses_not_invocation_claims() {
        let future = "struct Deferred<C>{ callback:Option<Box<C>> } impl<C> Deferred<C>{ fn new(factory:C)->Self{Self{callback:Some(Box::new(factory))}} } impl<C> Future for Deferred<C>{ fn poll(self:Pin<&mut Self>)->Poll<()> {let this=self.get_mut(); if let Some(f)=this.callback.take(){if cancelled(){return Poll::Ready(());} f();} Poll::Pending} }";
        let wrappers = "struct Scope; impl Scope { fn outer<C: FnOnce()>(factory:C)->Deferred<C>{Deferred::new(factory)} fn child<C: FnOnce()>(factory:C)->((),Deferred<C>){let future=Deferred::new(factory); ((),future)} fn ignored<C: FnOnce()>(factory:C){drop(factory);} }";
        let findings = inspect_forwarders(future, "future.rs", wrappers, "wrapper.rs").unwrap();
        assert_eq!(findings.len(), 2);
        assert!(findings.iter().any(|w| w.wrapper_method == "outer"));
        assert!(findings.iter().any(|w| w.wrapper_method == "child"));
        assert!(
            findings
                .iter()
                .all(|w| w.requires_poll_and_guard_fallthrough && w.poll.guard == "cancelled()")
        );
        let replaced = wrappers.replace(
            "Deferred::new(factory)",
            "{ let factory = replacement; Deferred::new(factory) }",
        );
        assert!(
            inspect_forwarders(future, "future.rs", &replaced, "wrapper.rs")
                .unwrap()
                .is_empty()
        );
        let ignored = future.replace("callback:Some(Box::new(factory))", "callback:None");
        assert!(
            inspect_forwarders(&ignored, "future.rs", wrappers, "wrapper.rs")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn target_forwarding_requires_matching_import_field_and_callback() {
        let future = "struct Deferred<C>{ callback:Option<Box<C>> } impl<C> Deferred<C>{ fn new(factory:C)->Self{Self{callback:Some(Box::new(factory))}} } impl<C> Future for Deferred<C>{ fn poll(self:Pin<&mut Self>)->Poll<()> {let this=self.get_mut(); if let Some(f)=this.callback.take(){if cancelled(){return Poll::Ready(());} f();} Poll::Pending} }";
        let wrapper = "struct Scope; impl Scope { fn outer<C: FnOnce()>(factory:C)->Deferred<C>{Deferred::new(factory)} }";
        let dependencies = inspect_forwarders(future, "future.rs", wrapper, "wrapper.rs").unwrap();
        let target = "use dep::Scope; struct Executor { cancellation: Scope } impl Executor { fn outer<C: FnOnce()>(self, factory:C) { self.cancellation.outer(move || factory()); } }";
        let findings =
            inspect_target_forwarders(target, "target.rs", "dep", &dependencies).unwrap();
        assert_eq!(findings.len(), 1);
        assert!(findings[0].requires_poll_and_guard_fallthrough);
        for negative in [
            target.replace("use dep::Scope", "use other::Scope"),
            target.replace("cancellation: Scope", "cancellation: Other"),
            target.replace("|| factory()", "|| ignored()"),
            target.replace(
                "self.cancellation.outer",
                "let factory = other; self.cancellation.outer",
            ),
            target.replace("|| factory()", "|| async { factory() }"),
        ] {
            assert!(
                inspect_target_forwarders(&negative, "target.rs", "dep", &dependencies)
                    .unwrap()
                    .is_empty()
            );
        }
    }
}
