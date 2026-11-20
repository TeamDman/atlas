//! Conservative call lookup through explicit imports, reexports and signature types.
//! Ambiguous, indirect and unsupported calls stay summarized; no Rust type inference.
use super::{Function, Index, Target};
use crate::{
    analysis::workflow::{Binding, Call},
    architecture::Kind,
};
use std::collections::{BTreeMap, BTreeSet};
use syn::Type;

pub(super) fn path(text: &str) -> Option<Vec<String>> {
    let path = syn::parse_str::<syn::Path>(text).ok()?;
    Some(path.segments.iter().map(|s| s.ident.to_string()).collect())
}
pub(super) fn ty(text: &str) -> Option<Vec<String>> {
    fn inner(ty: &Type) -> Option<Vec<String>> {
        match ty {
            Type::Reference(r) => inner(&r.elem),
            Type::Path(p) if p.qself.is_none() => Some(
                p.path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect(),
            ),
            _ => None,
        }
    }
    inner(&syn::parse_str(text).ok()?)
}
impl Index {
    pub(crate) fn scopes(&self, file: &str, module: &[String]) -> Vec<Vec<String>> {
        self.scopes
            .get(file)
            .into_iter()
            .flatten()
            .map(|scope| scope.iter().chain(module).cloned().collect())
            .collect()
    }
    pub(crate) fn in_scope(
        &self,
        scope: &[String],
        imports: &BTreeMap<String, Vec<Vec<String>>>,
        p: &[String],
    ) -> Option<Vec<String>> {
        let mut p = p.to_vec();
        if let Some(candidates) = p.first().and_then(|name| imports.get(name)) {
            let [import] = candidates.as_slice() else {
                return None;
            };
            p = import.iter().chain(&p[1..]).cloned().collect();
        }
        self.exported(&normalize(scope, &p, &self.crates))
    }
    pub(crate) fn canonical(&self, function: usize, p: &[String]) -> Option<Vec<String>> {
        let f = self.functions.get(function)?;
        let mut p = p.to_vec();
        if p.first()
            .is_some_and(|name| f.flow.type_parameters.contains(name))
        {
            return None;
        }
        if p.first().is_some_and(|name| name == "Self") {
            let mut owner = path(f.flow.owner.as_deref()?)?;
            owner.extend_from_slice(&p[1..]);
            p = owner;
        }
        self.in_scope(&f.scope, &f.flow.imports, &p)
    }

    pub(crate) fn exported(&self, path: &[String]) -> Option<Vec<String>> {
        let mut path = path.to_vec();
        let mut seen = BTreeSet::new();
        let mut aliases = BTreeSet::new();
        while seen.insert(path.clone()) {
            let found = (1..=path.len()).rev().find_map(|len| {
                self.exports
                    .get(&path[..len])
                    .map(|target| (len, target.clone()))
            });
            let Some((len, candidates)) = found else {
                return Some(path);
            };
            if !aliases.insert(path[..len].to_vec()) {
                return None;
            }
            let [target] = candidates.as_slice() else {
                return None;
            };
            let mut target = target.clone();
            target.extend_from_slice(&path[len..]);
            path = target;
        }
        None
    }
    fn classify(&self, p: &[String]) -> Target {
        let Some(resolved) = self.exported(p) else {
            return Target::Summarized("ambiguous or cyclic public reexport".into());
        };
        let p = resolved.as_slice();
        for (i, b) in self.boundaries.iter().enumerate() {
            if p.starts_with(&b.path) {
                let mut entry = b.path.clone();
                entry.push(b.module.entry.clone());
                let Some(entry) = self.exported(&entry) else {
                    return Target::Summarized("ambiguous stage entry reexport".into());
                };
                if matches!(b.module.kind, Kind::Instrument | Kind::Edge)
                    || (if self.names.contains_key(&entry) {
                        p == entry
                    } else {
                        p.len() == entry.len() + 1 && p.starts_with(&entry)
                    })
                {
                    let member = if matches!(b.module.kind, Kind::Instrument | Kind::Edge) {
                        Some(p[b.path.len()..].join("::"))
                    } else if p.len() > entry.len() {
                        Some(p[entry.len()..].join("::"))
                    } else {
                        None
                    };
                    return Target::Boundary(i, member);
                }
                return Target::Summarized(format!(
                    "{} support API, outside its declared entry",
                    b.module.name
                ));
            }
        }
        match self.names.get(p).map(Vec::as_slice) {
            Some([i]) => Target::Function(*i),
            Some(candidates) => Target::Summarized(format!(
                "ambiguous static call: {} candidates",
                candidates.len()
            )),
            None => self.glob_function_candidate(p).unwrap_or_else(|| {
                Target::Summarized(format!("unresolved or external path {}", p.join("::")))
            }),
        }
    }
    /// A public glob may expose a free function from an observed source module.
    /// A single written match is only a candidate: conditional compilation,
    /// macros and unobserved exports can change the actual namespace.
    fn glob_function_candidate(&self, path: &[String]) -> Option<Target> {
        let (name, parent) = path.split_last()?;
        let found = self.glob_function_candidates(parent, name, &mut BTreeSet::new())?;
        if found.len() == 1 {
            Some(Target::Candidate(
                *found.first()?,
                "public glob reexport of a unique written free function; conditional, macro-generated and external competitors are not ruled out".into(),
            ))
        } else {
            None
        }
    }
    fn glob_function_candidates(
        &self,
        parent: &[String],
        name: &str,
        visited: &mut BTreeSet<Vec<String>>,
    ) -> Option<BTreeSet<usize>> {
        if !visited.insert(parent.to_vec()) {
            return None;
        }
        let mut star = parent.to_vec();
        star.push("*".into());
        let modules = self.exports.get(&star)?;
        let mut found = BTreeSet::new();
        for module in modules {
            let module = self.exported(module)?;
            if !self.scopes.values().flatten().any(|scope| scope == &module) {
                return None;
            }
            let mut candidate = module.clone();
            candidate.push(name.to_owned());
            let candidate = self.exported(&candidate)?;
            if let Some(positions) = self.names.get(&candidate) {
                for position in positions {
                    if self.functions[*position].flow.public_free_function {
                        found.insert(*position);
                    }
                }
            }
            let mut nested_star = module.clone();
            nested_star.push("*".into());
            if self.exports.contains_key(&nested_star) {
                found.extend(self.glob_function_candidates(&module, name, visited)?);
            }
        }
        visited.remove(parent);
        Some(found)
    }
    /// Resolve a call in a function selected from this index. Positions from
    /// another index have no identity here, even when numerically in range.
    /// An out-of-range position returns Target::InvalidFunction.
    pub fn resolve(&self, function: usize, call: &Call) -> Target {
        let Some(f) = self.functions.get(function) else {
            return Target::InvalidFunction(function);
        };
        if let Some(receiver) = &call.receiver {
            return self.resolve_receiver(function, f, receiver, call);
        }
        let Some(p) = path(&call.spelling) else {
            return Target::Summarized("indirect or unsupported call syntax".into());
        };
        if p.len() == 1
            && f.flow.bindings.iter().any(|b| {
                b.name == p[0]
                    && (b.site.end_line, b.site.end_column) <= (call.site.line, call.site.column)
            })
        {
            return Target::Summarized(format!("value call {}", p[0]));
        }
        if shadowed(&f.flow.local_names, &p[0]) {
            return Target::Summarized(format!(
                "block-local item or import {} shadows the module's names; not resolved",
                p[0]
            ));
        }
        if p.len() == 1 && f.flow.imports.contains_key(&p[0]) {
            let local = normalize(&f.scope, &p, &self.crates);
            if self.names.contains_key(&local) {
                return Target::Summarized(
                    "local value and import share a name; namespace unresolved".into(),
                );
            }
        }
        if call.awaited_block_call {
            let target = self
                .canonical(function, &p)
                .map(|path| self.classify(&path));
            return match target {
                Some(Target::Function(index) | Target::Candidate(index, _))
                    if self.functions[index].flow.async_function => Target::Candidate(index, "directly awaited source path with a unique written async body; macro-generated and external alternatives are not ruled out".into()),
                _ => Target::Summarized("awaited-block path has no unique written async body".into()),
            };
        }
        self.canonical(function, &p).map_or_else(
            || Target::Summarized("ambiguous source import".into()),
            |p| self.classify(&p),
        )
    }
    fn resolve_receiver(
        &self,
        function: usize,
        f: &Function,
        receiver: &str,
        call: &Call,
    ) -> Target {
        if call.awaited_block_call {
            if receiver != "self" {
                return Target::Summarized("unsupported awaited-block receiver".into());
            }
            let target = self.canonical(function, &["Self".into(), call.spelling.clone()]);
            return match target.map(|p| self.classify(&p)) {
                Some(Target::Function(index))
                    if (self.functions[index].inherent || self.functions[index].generic_inherent)
                        && self.functions[index].flow.async_function =>
                {
                    Target::Candidate(index, "directly awaited self call in an awaited async block with a unique written inherent async body; macro-generated and external alternatives are not ruled out".into())
                }
                _ => Target::Summarized("awaited-block self call has no unique written inherent body".into()),
            };
        }
        if let Ok(expr) = syn::parse_str::<syn::Expr>(receiver) {
            match expr {
                syn::Expr::Struct(s)
                    if s.qself.is_none()
                        && s.path
                            .segments
                            .iter()
                            .all(|s| matches!(s.arguments, syn::PathArguments::None)) =>
                {
                    let parts: Vec<_> = s
                        .path
                        .segments
                        .iter()
                        .map(|s| s.ident.to_string())
                        .collect();
                    if parts
                        .first()
                        .is_some_and(|first| shadowed(&f.flow.local_names, first))
                    {
                        return Target::Summarized(format!(
                            "receiver {receiver}: local type name may be shadowed"
                        ));
                    }
                    if let Some(owner) = self.canonical(function, &parts) {
                        return self.written_struct_method(function, &owner, call, receiver);
                    }
                }
                syn::Expr::Field(field) if matches!(&*field.base, syn::Expr::Path(p) if p.path.is_ident("self")) => {
                    if let syn::Member::Named(name) = field.member
                        && let Some(owner) = self.canonical(function, &["Self".into()])
                        && let Some([shape]) = self.structs.get(&owner).map(Vec::as_slice)
                        && shape.supported
                        && let Some(field_type) = shape.fields.get(&name.to_string())
                        && let Some(field_owner) =
                            self.in_scope(&shape.scope, &shape.imports, field_type)
                    {
                        return self.written_struct_method(function, &field_owner, call, receiver);
                    }
                }
                _ => {}
            }
        }
        if receiver == "self" {
            return self
                .canonical(function, &["Self".into(), call.spelling.clone()])
                .map_or_else(
                    || Target::Summarized("self owner not established".into()),
                    |p| self.classify(&p),
                );
        }
        let bindings: Vec<_> = f
            .flow
            .bindings
            .iter()
            .filter(|b| {
                b.name == receiver
                    && (b.site.end_line, b.site.end_column) <= (call.site.line, call.site.column)
            })
            .collect();
        if bindings.len() != 1 {
            return Target::Summarized(format!(
                "receiver {receiver}: no unique explicit source type"
            ));
        }
        let owner = self.binding_owner(function, bindings[0]);
        owner.map_or_else(
            || {
                Target::Summarized(format!(
                    "receiver {receiver}: initializer result not established"
                ))
            },
            |mut p| {
                p.push(call.spelling.clone());
                self.classify(&p)
            },
        )
    }

    fn written_struct_method(
        &self,
        function: usize,
        owner: &[String],
        call: &Call,
        receiver: &str,
    ) -> Target {
        let Some(owner) = self.struct_path(owner) else {
            return Target::Summarized(format!(
                "receiver {receiver}: struct declaration not unique"
            ));
        };
        let Some([shape]) = self.structs.get(&owner).map(Vec::as_slice) else {
            return Target::Summarized(format!(
                "receiver {receiver}: struct declaration not unique"
            ));
        };
        if !shape.supported {
            return Target::Summarized(format!(
                "receiver {receiver}: generic or conditional struct"
            ));
        }
        let mut name = owner.clone();
        name.push(call.spelling.clone());
        match self.classify(&name) {
            Target::Function(index) if self.functions[index].inherent => Target::Function(index),
            boundary @ Target::Boundary(..) => boundary,
            _ if self.names.get(&name).is_some_and(|candidates| {
                candidates.iter().any(|i| self.functions[*i].inherent)
            }) =>
            {
                Target::Summarized(format!(
                    "receiver {receiver}: competing inherent method observations"
                ))
            }
            _ => self.trait_method_candidate(function, &owner, call, receiver),
        }
    }

    fn trait_method_candidate(
        &self,
        function: usize,
        owner: &[String],
        call: &Call,
        receiver: &str,
    ) -> Target {
        let f = &self.functions[function];
        if f.flow.imports.contains_key("*")
            || f.flow.local_names.iter().any(|n| n == "*")
            || f.flow.imports.values().any(|paths| paths.len() != 1)
        {
            return Target::Summarized(format!(
                "receiver {receiver}: glob import leaves trait scope unknown"
            ));
        }
        let mut visible = BTreeSet::new();
        for name in f.flow.imports.keys() {
            if shadowed(&f.flow.local_names, name) {
                continue;
            }
            if let Some(path) = self.canonical(function, std::slice::from_ref(name))
                && self.traits.contains_key(&path)
            {
                visible.insert(path);
            }
        }
        for path in self.traits.keys() {
            if path.len() == f.scope.len() + 1
                && path.starts_with(&f.scope)
                && !shadowed(&f.flow.local_names, &path[path.len() - 1])
            {
                visible.insert(path.clone());
            }
        }
        let mut candidates = Vec::new();
        for trait_path in visible {
            let Some(shapes) = self.traits.get(&trait_path) else {
                continue;
            };
            let [shape] = shapes.as_slice() else {
                return Target::Summarized("ambiguous trait declaration".into());
            };
            let Some(methods) = shape.methods.get(&call.spelling) else {
                continue;
            };
            let [default] = methods.as_slice() else {
                return Target::Summarized("ambiguous trait method".into());
            };
            if !shape.supported {
                return Target::Summarized("conditional or generic trait method".into());
            }
            for implementation in &self.impls {
                if self
                    .in_scope(
                        &implementation.scope,
                        &implementation.imports,
                        &implementation.trait_path,
                    )
                    .as_deref()
                    != Some(&trait_path)
                {
                    continue;
                }
                if implementation.generic_owner {
                    return Target::Summarized("generic trait implementation may apply".into());
                }
                let Some(impl_owner) = self
                    .in_scope(
                        &implementation.scope,
                        &implementation.imports,
                        &implementation.owner,
                    )
                    .and_then(|p| self.struct_path(&p))
                else {
                    continue;
                };
                if impl_owner != owner {
                    continue;
                }
                if !implementation.supported {
                    return Target::Summarized(
                        "conditional or generic trait implementation".into(),
                    );
                }
                let selected = if let Some(overrides) = implementation.methods.get(&call.spelling) {
                    let [site] = overrides.as_slice() else {
                        return Target::Summarized("ambiguous trait override".into());
                    };
                    Some((site, false))
                } else {
                    default.as_ref().map(|site| (site, true))
                };
                if let Some((site, is_default)) = selected {
                    let matches: Vec<_> = self
                        .functions
                        .iter()
                        .enumerate()
                        .filter(|(_, body)| {
                            &body.flow.site == site && body.default_body == is_default
                        })
                        .map(|(index, _)| index)
                        .collect();
                    let [index] = matches.as_slice() else {
                        return Target::Summarized(
                            "trait body has ambiguous source contexts".into(),
                        );
                    };
                    candidates.push((*index, format!("{} via impl at {}; other external or macro-generated candidates are not ruled out", trait_path.join("::"), implementation.site.link())));
                }
            }
        }
        match candidates.as_slice() {
            [(index, reason)] => Target::Candidate(*index, reason.clone()),
            [] => Target::Summarized(format!(
                "receiver {receiver}: no unique supported in-scope trait method"
            )),
            _ => Target::Summarized(format!(
                "receiver {receiver}: ambiguous in-scope trait candidates"
            )),
        }
    }

    fn struct_path(&self, path: &[String]) -> Option<Vec<String>> {
        if let Some(shapes) = self.structs.get(path) {
            let [shape] = shapes.as_slice() else {
                return None;
            };
            return shape.supported.then(|| path.to_vec());
        }
        let (name, parent) = path.split_last()?;
        let mut star = parent.to_vec();
        star.push("*".into());
        let globs = self.exports.get(&star)?;
        let mut found = BTreeSet::new();
        for module in globs {
            let module = self.exported(module)?;
            if !self.scopes.values().flatten().any(|scope| scope == &module) {
                return None;
            }
            let mut candidate = module;
            candidate.push(name.clone());
            if let Some(shapes) = self.structs.get(&candidate) {
                let [shape] = shapes.as_slice() else {
                    return None;
                };
                if shape.public && shape.supported {
                    found.insert(candidate);
                }
            }
        }
        if found.len() == 1 {
            found.pop_first()
        } else {
            None
        }
    }

    /// Prefer the written type; otherwise inspect a unique initializer's return type.
    fn binding_owner(&self, function: usize, binding: &Binding) -> Option<Vec<String>> {
        if let Some(written) = binding.ty.as_deref().and_then(ty) {
            return self.canonical(function, &written);
        }
        let initializer = path(binding.initializer.as_deref()?)?;
        let canonical = self.canonical(function, &initializer)?;
        let exported = self.exported(&canonical)?;
        let [target] = self.names.get(&exported)?.as_slice() else {
            return None;
        };
        match self.functions[*target].flow.output.as_str() {
            "Self" => Some(exported[..exported.len() - 1].to_vec()),
            output => self.canonical(*target, &ty(output)?),
        }
    }
}

pub(super) fn normalize(
    scope: &[String],
    path: &[String],
    crates: &BTreeSet<String>,
) -> Vec<String> {
    let mut path = path.to_vec();
    if path.first().is_some_and(|name| crates.contains(name)) {
        return path;
    }
    let mut scope = scope.to_vec();
    if path.first().is_some_and(|name| name == "crate") {
        scope.truncate(1);
        path.remove(0);
    } else if path.first().is_some_and(|name| name == "self") {
        path.remove(0);
    } else {
        while path.first().is_some_and(|name| name == "super") {
            scope.pop();
            path.remove(0);
        }
    }
    scope.extend(path);
    scope
}

/// Whether a body-local item or import may bind a path's first segment: by
/// name, or through a body-local glob unless the segment is a path keyword.
fn shadowed(local_names: &[String], first: &str) -> bool {
    local_names.iter().any(|name| name == first)
        || (local_names.iter().any(|name| name == "*")
            && !matches!(first, "crate" | "self" | "super" | "Self"))
}
