//! Conservative call lookup through explicit imports, reexports and signature types.
//! Ambiguous, indirect and unsupported calls stay summarized; no Rust type inference.
use super::{AwaitSubstitution, Function, Index, Target};
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
    /// Refuse visible conflicts with the written, possibly prelude, FnOnce name.
    pub(crate) fn callback_bound_unshadowed(&self, function: usize) -> bool {
        let Some(f) = self.functions.get(function) else {
            return false;
        };
        if f.flow
            .local_names
            .iter()
            .any(|name| name == "FnOnce" || name == "*")
            || f.flow.imports.contains_key("*")
        {
            return false;
        }
        if let Some(imports) = f.flow.imports.get("FnOnce") {
            let [import] = imports.as_slice() else {
                return false;
            };
            if import.as_slice() != ["std", "ops", "FnOnce"]
                && import.as_slice() != ["core", "ops", "FnOnce"]
            {
                return false;
            }
        }
        for length in 1..=f.scope.len() {
            let mut path = f.scope[..length].to_vec();
            path.push("FnOnce".into());
            if self.traits.contains_key(&path) || self.structs.contains_key(&path) {
                return false;
            }
        }
        true
    }
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
        if let Some(name) = &call.awaited_value {
            return Target::Summarized(format!(
                "awaited callback {name}: source argument not mapped"
            ));
        }
        if let Some(producer) = &call.await_producer {
            return self.resolve_into_future(function, producer);
        }
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
    /// Match only a written `Wrapper<'a, Self>` result from a resolved method
    /// on a written struct literal to a unique local `IntoFuture for Wrapper<R>`.
    /// Bounds and external/macro-generated implementations remain unproved.
    fn resolve_into_future(
        &self,
        function: usize,
        producer: &crate::analysis::workflow::Site,
    ) -> Target {
        self.find_into_future_candidate(function, producer)
            .map_or_else(
                || {
                    Target::Summarized(
                        "implicit IntoFuture not uniquely evidenced; polling remains opaque".into(),
                    )
                },
                |(target, _)| target,
            )
    }
    fn find_into_future_candidate(
        &self,
        function: usize,
        producer: &crate::analysis::workflow::Site,
    ) -> Option<(Target, AwaitSubstitution)> {
        let caller = self.functions.get(function)?;
        let mut calls = caller
            .flow
            .calls
            .iter()
            .filter(|call| &call.site == producer && call.await_producer.is_none());
        let call = calls.next()?;
        if calls.next().is_some() {
            return None;
        }
        let syn::Expr::Struct(receiver) =
            syn::parse_str::<syn::Expr>(call.receiver.as_deref()?).ok()?
        else {
            return None;
        };
        if receiver.qself.is_some()
            || receiver
                .path
                .segments
                .iter()
                .any(|s| !matches!(s.arguments, syn::PathArguments::None))
        {
            return None;
        }
        let parts: Vec<_> = receiver
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect();
        let concrete = self.struct_path(&self.canonical(function, &parts)?)?;
        let (Target::Function(producer_index) | Target::Candidate(producer_index, _)) =
            self.resolve(function, call)
        else {
            return None;
        };
        let producer_flow = &self.functions.get(producer_index)?.flow;
        if producer_flow.async_function {
            return None;
        }
        let producer_trait = if self.functions[producer_index].default_body {
            let trait_name = producer_flow
                .function
                .rsplit_once("::")?
                .0
                .rsplit("::")
                .next()?;
            self.canonical(producer_index, &[trait_name.to_owned()])
        } else {
            None
        };
        let syn::Type::Path(output) = syn::parse_str::<syn::Type>(&producer_flow.output).ok()?
        else {
            return None;
        };
        if output.qself.is_some() {
            return None;
        }
        let last = output.path.segments.last()?;
        if output
            .path
            .segments
            .iter()
            .take(output.path.segments.len().saturating_sub(1))
            .any(|segment| !matches!(segment.arguments, syn::PathArguments::None))
        {
            return None;
        }
        let syn::PathArguments::AngleBracketed(arguments) = &last.arguments else {
            return None;
        };
        let mut types = Vec::new();
        for argument in &arguments.args {
            match argument {
                syn::GenericArgument::Lifetime(_) => {}
                syn::GenericArgument::Type(ty) => types.push(ty),
                _ => return None,
            }
        }
        let [syn::Type::Path(argument)] = types.as_slice() else {
            return None;
        };
        if argument.qself.is_some() || !argument.path.is_ident("Self") {
            return None;
        }
        let outer: Vec<_> = output
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect();
        let owner = self.canonical(producer_index, &outer)?;
        let mut found = Vec::new();
        for implementation in &self.impls {
            let known = is_standard_future_trait(self, implementation, "IntoFuture");
            let future = is_standard_future_trait(self, implementation, "Future");
            let uncertain_glob = !known
                && (implementation.trait_path == ["IntoFuture"]
                    || implementation.trait_path == ["Future"])
                && implementation.imports.contains_key("*");
            if !known && !future && !uncertain_glob {
                continue;
            }
            if implementation.generic_owner {
                return None;
            }
            let impl_owner = self.in_scope(
                &implementation.scope,
                &implementation.imports,
                &implementation.owner,
            )?;
            if impl_owner != owner {
                continue;
            }
            if uncertain_glob || future {
                return None;
            }
            let parameter = implementation.future_parameter.as_ref()?;
            let bounds = implementation.future_bounds.as_ref()?;
            for bound in &bounds.traits {
                let bound = self.in_scope(&implementation.scope, &implementation.imports, bound)?;
                if producer_trait.as_ref() != Some(&bound) {
                    return None;
                }
            }
            if !bounds.associated_types.is_empty()
                && !self.witnessed_associated_types(
                    producer_trait.as_deref()?,
                    &concrete,
                    &bounds.associated_types,
                )
            {
                return None;
            }
            if !implementation.future_method_ok {
                return None;
            }
            let [site] = implementation.methods.get("into_future")?.as_slice() else {
                return None;
            };
            let mut matches = self
                .functions
                .iter()
                .enumerate()
                .filter(|(_, f)| &f.flow.site == site && !f.default_body)
                .map(|(i, _)| i);
            let index = matches.next()?;
            if matches.next().is_some() {
                return None;
            }
            found.push((index, format!("IntoFuture impl at {}; {} = {}; written trait bounds matched; lifetimes, external impls and macro effects not proved", implementation.site.link(), parameter, concrete.join("::")), AwaitSubstitution { owner: owner.clone(), parameter: parameter.clone(), concrete: concrete.clone(), trait_path: producer_trait.clone() }));
        }
        match found.as_slice() {
            [(index, reason, substitution)] => Some((
                Target::Candidate(*index, reason.clone()),
                substitution.clone(),
            )),
            _ => None,
        }
    }
    pub(crate) fn await_substitution(
        &self,
        function: usize,
        call: &Call,
    ) -> Option<AwaitSubstitution> {
        self.find_into_future_candidate(function, call.await_producer.as_ref()?)
            .map(|(_, substitution)| substitution)
    }
    fn witnessed_associated_types(
        &self,
        trait_path: &[String],
        concrete: &[String],
        required: &BTreeSet<String>,
    ) -> bool {
        let Some([shape]) = self.traits.get(trait_path).map(Vec::as_slice) else {
            return false;
        };
        if !shape.supported || !required.is_subset(&shape.associated_types) {
            return false;
        }
        let mut matches = Vec::new();
        for implementation in &self.impls {
            if self
                .in_scope(
                    &implementation.scope,
                    &implementation.imports,
                    &implementation.trait_path,
                )
                .as_deref()
                != Some(trait_path)
            {
                continue;
            }
            if implementation.generic_owner {
                return false;
            }
            let owner = self
                .in_scope(
                    &implementation.scope,
                    &implementation.imports,
                    &implementation.owner,
                )
                .and_then(|path| self.struct_path(&path));
            if owner.as_deref() == Some(concrete) {
                if !implementation.supported {
                    return false;
                }
                matches.push(implementation);
            }
        }
        matches.as_slice().first().is_some_and(|implementation| {
            matches.len() == 1 && required.is_subset(&implementation.associated_types)
        })
    }
    pub(crate) fn same_substituted_owner(
        &self,
        caller: usize,
        callee: usize,
        substitution: &AwaitSubstitution,
    ) -> bool {
        let Some(a) = self.functions.get(caller) else {
            return false;
        };
        let Some(b) = self.functions.get(callee) else {
            return false;
        };
        a.flow.owner.is_some()
            && a.flow.owner == b.flow.owner
            && a.flow.type_parameters.contains(&substitution.parameter)
            && b.flow.type_parameters.contains(&substitution.parameter)
            && self.canonical(caller, &["Self".into()]).as_ref() == Some(&substitution.owner)
            && self.canonical(callee, &["Self".into()]).as_ref() == Some(&substitution.owner)
    }
    /// Resolve a direct generic field through the concrete type already
    /// witnessed at an await. This follows the written bound, not a same-named
    /// inherent method on the concrete type; external/macro cases remain open.
    pub(crate) fn resolve_substituted_field(
        &self,
        function: usize,
        call: &Call,
        substitution: &AwaitSubstitution,
    ) -> Option<Target> {
        if !self.same_substituted_owner(function, function, substitution) {
            return None;
        }
        let syn::Expr::Field(field) =
            syn::parse_str::<syn::Expr>(call.receiver.as_deref()?).ok()?
        else {
            return None;
        };
        if !matches!(&*field.base, syn::Expr::Path(path) if path.qself.is_none() && path.path.is_ident("self"))
        {
            return None;
        }
        let syn::Member::Named(name) = field.member else {
            return None;
        };
        let [shape] = self.structs.get(&substitution.owner)?.as_slice() else {
            return None;
        };
        let parameter = shape.generic_parameter.as_ref()?;
        if shape.ambiguous_fields.contains(&name.to_string())
            || shape.fields.get(&name.to_string())?.as_slice() != [parameter.as_str()]
        {
            return None;
        }
        if !self.owner_uses_substitution(function, substitution) {
            return None;
        }
        let f = self.functions.get(function)?;
        let trait_path = substitution.trait_path.as_deref()?;
        let bounds = f.flow.generic_bounds.get(&substitution.parameter)?;
        if bounds.iter().any(Vec::is_empty) {
            return None;
        }
        if !bounds
            .iter()
            .any(|bound| self.canonical(function, bound).as_deref() == Some(trait_path))
        {
            return None;
        }
        for bound in bounds {
            if let Some(path) = self.canonical(function, bound)
                && path != trait_path
                && self.traits.get(&path).is_some_and(|shapes| {
                    shapes
                        .iter()
                        .any(|shape| shape.methods.contains_key(&call.spelling))
                })
            {
                return None;
            }
        }
        Some(self.trait_method_candidate(
            function,
            &substitution.concrete,
            call,
            call.receiver.as_deref()?,
            Some(trait_path),
        ))
    }
    fn owner_uses_substitution(&self, function: usize, substitution: &AwaitSubstitution) -> bool {
        self.owner_uses_substitution_inner(function, substitution)
            .is_some()
    }
    fn owner_uses_substitution_inner(
        &self,
        function: usize,
        substitution: &AwaitSubstitution,
    ) -> Option<()> {
        if !self.same_substituted_owner(function, function, substitution) {
            return None;
        }
        let f = self.functions.get(function)?;
        let syn::Type::Path(owner) = syn::parse_str::<syn::Type>(f.flow.owner.as_deref()?).ok()?
        else {
            return None;
        };
        let syn::PathArguments::AngleBracketed(arguments) = &owner.path.segments.last()?.arguments
        else {
            return None;
        };
        let mut types = Vec::new();
        for argument in &arguments.args {
            match argument {
                syn::GenericArgument::Type(ty) => types.push(ty),
                syn::GenericArgument::Lifetime(_) => {}
                _ => return None,
            }
        }
        let [syn::Type::Path(argument)] = types.as_slice() else {
            return None;
        };
        if argument.qself.is_some() || !argument.path.is_ident(&substitution.parameter) {
            return None;
        }
        Some(())
    }
    /// Follow a directly returned pinned async inherent self-call only as a
    /// source candidate; the caller must place its body behind a poll gate.
    pub(crate) fn returned_pinned_async_target(
        &self,
        function: usize,
        substitution: &AwaitSubstitution,
    ) -> Option<(usize, crate::analysis::workflow::Site)> {
        if !self.owner_uses_substitution(function, substitution) {
            return None;
        }
        let f = self.functions.get(function)?;
        if f.flow.imports.contains_key("std") || shadowed(&f.flow.local_names, "std") {
            return None;
        }
        for length in 1..=f.scope.len() {
            let mut path = f.scope[..length].to_vec();
            path.push("std".into());
            if self.scopes.values().flatten().any(|scope| scope == &path)
                || self.traits.keys().any(|item| item.starts_with(&path))
                || self.structs.keys().any(|item| item.starts_with(&path))
            {
                return None;
            }
        }
        let site = f.flow.returned_pinned_self_call.as_ref()?;
        let mut calls = f
            .flow
            .calls
            .iter()
            .filter(|call| &call.site == site && call.receiver.as_deref() == Some("self"));
        let call = calls.next()?;
        if calls.next().is_some() {
            return None;
        }
        let Target::Function(target) = self.resolve(function, call) else {
            return None;
        };
        let body = self.functions.get(target)?;
        if !body.flow.async_function
            || !body.generic_owner_inherent
            || !self.owner_uses_substitution(target, substitution)
        {
            return None;
        }
        Some((target, site.clone()))
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
                syn::Expr::MethodCall(_) => {
                    let mut guards = Vec::new();
                    if let Some(owner) =
                        self.builder_expression_owner(function, &expr, &call.site, 0, &mut guards)
                    {
                        return self.builder_candidate(function, &owner, call, receiver, &guards);
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
                if bindings[0]
                    .initializer
                    .as_deref()
                    .is_some_and(|s| syn::parse_str::<syn::ExprMethodCall>(s).is_ok())
                {
                    let mut guards = Vec::new();
                    if let Some(initial) = bindings[0].initializer.as_deref()
                        && let Ok(expr) = syn::parse_str::<syn::Expr>(initial)
                    {
                        self.builder_expression_owner(
                            function,
                            &expr,
                            &bindings[0].site,
                            0,
                            &mut guards,
                        );
                    }
                    return self.builder_candidate(function, &p, call, receiver, &guards);
                }
                p.push(call.spelling.clone());
                self.classify(&p)
            },
        )
    }

    fn builder_candidate(
        &self,
        function: usize,
        owner: &[String],
        call: &Call,
        receiver: &str,
        guards: &[String],
    ) -> Target {
        let Some([shape]) = self.structs.get(owner).map(Vec::as_slice) else {
            return Target::Summarized(format!(
                "receiver {receiver}: struct declaration not unique"
            ));
        };
        if !shape.supported {
            return Target::Summarized(format!(
                "receiver {receiver}: generic or conditional struct"
            ));
        }
        let mut name = owner.to_vec();
        name.push(call.spelling.clone());
        match self.classify(&name) {
            Target::Function(index)
                if self.functions[index].inherent
                    || self.functions[index].generic_inherent
                    || self.functions[index].conditional_inherent.is_some() =>
            {
                let mut conditions = guards.to_vec();
                if let Some(guard) = &self.functions[index].conditional_inherent {
                    conditions.push(guard.clone());
                }
                Target::Candidate(
                    index,
                    format!(
                        "receiver follows uniquely written constructor and Self-returning inherent methods; source candidate, not compiler inference; macro, generic constraints, and external alternatives remain unknown{}",
                        if conditions.is_empty() {
                            String::new()
                        } else {
                            format!("; conditional on {}", conditions.join(" and "))
                        }
                    ),
                )
            }
            _ => self.written_struct_method(function, owner, call, receiver),
        }
    }

    /// Follow only bounded chains whose unique written methods return `Self`.
    /// This records a source candidate and does not prove overload selection.
    fn builder_expression_owner(
        &self,
        function: usize,
        expr: &syn::Expr,
        before: &crate::analysis::workflow::Site,
        depth: usize,
        guards: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        if depth >= 12 {
            return None;
        }
        match expr {
            syn::Expr::Paren(inner) => {
                self.builder_expression_owner(function, &inner.expr, before, depth + 1, guards)
            }
            syn::Expr::Group(inner) => {
                self.builder_expression_owner(function, &inner.expr, before, depth + 1, guards)
            }
            syn::Expr::Path(value) if value.qself.is_none() => {
                let name = value.path.get_ident()?.to_string();
                let f = self.functions.get(function)?;
                let bindings: Vec<_> = f
                    .flow
                    .bindings
                    .iter()
                    .filter(|binding| {
                        binding.name == name
                            && (binding.site.end_line, binding.site.end_column)
                                <= (before.line, before.column)
                    })
                    .collect();
                let [binding] = bindings.as_slice() else {
                    return None;
                };
                if let Some(owner) = binding
                    .ty
                    .as_deref()
                    .and_then(ty)
                    .and_then(|written| self.canonical(function, &written))
                {
                    return Some(owner);
                }
                let initial = binding.initializer.as_deref()?;
                if let Ok(expression) = syn::parse_str::<syn::Expr>(initial) {
                    return self.builder_expression_owner(
                        function,
                        &expression,
                        &binding.site,
                        depth + 1,
                        guards,
                    );
                }
                None
            }
            syn::Expr::Call(call) => {
                let syn::Expr::Path(path) = &*call.func else {
                    return None;
                };
                if path.qself.is_some() {
                    return None;
                }
                let parts: Vec<_> = path
                    .path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect();
                let f = self.functions.get(function)?;
                if parts
                    .first()
                    .is_some_and(|first| shadowed(&f.flow.local_names, first))
                {
                    return None;
                }
                let name = self.canonical(function, &parts)?;
                let Target::Function(target) = self.classify(&name) else {
                    return None;
                };
                let body = self.functions.get(target)?;
                if body.flow.output != "Self" {
                    return None;
                }
                self.canonical(target, &["Self".into()])
            }
            syn::Expr::MethodCall(call) => {
                let owner = self.builder_expression_owner(
                    function,
                    &call.receiver,
                    before,
                    depth + 1,
                    guards,
                )?;
                let Some([shape]) = self.structs.get(&owner).map(Vec::as_slice) else {
                    return None;
                };
                if !shape.supported {
                    return None;
                }
                let mut name = owner.clone();
                name.push(call.method.to_string());
                let Target::Function(target) = self.classify(&name) else {
                    return None;
                };
                let body = self.functions.get(target)?;
                if !(body.inherent || body.generic_inherent || body.conditional_inherent.is_some())
                    || body.flow.output != "Self"
                    || self.canonical(target, &["Self".into()]).as_ref() != Some(&owner)
                {
                    return None;
                }
                if let Some(guard) = &body.conditional_inherent {
                    guards.push(guard.clone());
                }
                Some(owner)
            }
            _ => None,
        }
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
            _ => self.trait_method_candidate(function, &owner, call, receiver, None),
        }
    }

    fn trait_method_candidate(
        &self,
        function: usize,
        owner: &[String],
        call: &Call,
        receiver: &str,
        constrained_trait: Option<&[String]>,
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
        if let Some(constrained) = constrained_trait {
            // The written generic bound itself is a scope witness, even when
            // the trait is named by a fully qualified path rather than a use.
            visible.clear();
            visible.insert(constrained.to_vec());
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
        let value = binding.initializer.as_deref()?;
        if let Ok(expression @ syn::Expr::MethodCall(_)) = syn::parse_str::<syn::Expr>(value) {
            return self.builder_expression_owner(
                function,
                &expression,
                &binding.site,
                0,
                &mut Vec::new(),
            );
        }
        let initializer = path(value)?;
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

fn is_standard_future_trait(
    index: &Index,
    implementation: &super::receivers::TraitImpl,
    name: &str,
) -> bool {
    let path = if let [name] = implementation.trait_path.as_slice() {
        let Some([import]) = implementation.imports.get(name).map(Vec::as_slice) else {
            return false;
        };
        import.as_slice()
    } else {
        implementation.trait_path.as_slice()
    };
    if path != ["std", "future", name] && path != ["core", "future", name] {
        return false;
    }
    let root = &path[0];
    if implementation.imports.contains_key(root) {
        return false;
    }
    for length in 1..=implementation.scope.len() {
        let mut local = implementation.scope[..length].to_vec();
        local.push(root.clone());
        if index.scopes.values().flatten().any(|scope| scope == &local)
            || index.traits.keys().any(|path| path.starts_with(&local))
        {
            return false;
        }
    }
    true
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

#[cfg(test)]
mod workspace_probe {
    use super::*;
    use syn::{spanned::Spanned, visit::Visit};

    #[test]
    #[ignore = "requires an explicitly selected external workspace and source locations"]
    fn inspect_deferred_field_with_await_substitution() {
        let env = |name| std::env::var(name).expect("probe environment");
        let root = std::path::PathBuf::from(env("ATLAS_PROBE_ROOT"));
        let caller_path = env("ATLAS_PROBE_CALLER_PATH");
        let caller_selector = env("ATLAS_PROBE_CALLER_FUNCTION");
        let target_path = env("ATLAS_PROBE_TARGET_PATH");
        let target_selector = env("ATLAS_PROBE_TARGET_FUNCTION");
        let expected_path = env("ATLAS_PROBE_EXPECTED_PATH");
        let expected_selector = env("ATLAS_PROBE_EXPECTED_FUNCTION");
        let line: usize = env("ATLAS_PROBE_CALL_LINE").parse().expect("call line");
        let metadata = cargo_metadata::MetadataCommand::new()
            .current_dir(&root)
            .no_deps()
            .exec()
            .expect("metadata");
        let graph = crate::discovery::scan_metadata(&metadata).expect("discovery");
        let architecture = crate::architecture::Architecture {
            schema: 1,
            provenance: vec![],
            modules: vec![],
            workflows: vec![],
        };
        let index = Index::new(&root, &metadata, &graph, &architecture).expect("index");
        let caller = index
            .select(&caller_path, &caller_selector)
            .expect("caller");
        let substitutions: Vec<_> = index.functions()[caller]
            .flow
            .calls
            .iter()
            .filter_map(|call| index.await_substitution(caller, call))
            .collect();
        let [substitution] = substitutions.as_slice() else {
            panic!("expected exactly one substitution");
        };
        let function = index
            .select(&target_path, &target_selector)
            .expect("generic function");
        let source = std::fs::read_to_string(root.join(&target_path)).expect("source");
        let syntax = syn::parse_file(&source).expect("parse source");
        struct Find(usize, Vec<syn::ExprMethodCall>);
        impl<'ast> Visit<'ast> for Find {
            fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
                if call.method.span().start().line == self.0 {
                    self.1.push(call.clone());
                }
                syn::visit::visit_expr_method_call(self, call);
            }
        }
        let mut find = Find(line, vec![]);
        find.visit_file(&syntax);
        let [call] = find.1.as_slice() else {
            panic!("expected exactly one method call at line");
        };
        let call = Call {
            spelling: call.method.to_string(),
            receiver: Some(crate::source_text::text(&source, call.receiver.span())),
            arguments: vec![],
            site: crate::analysis::workflow::Site::at(&target_path, call.span()),
            awaited_block_call: false,
            await_producer: None,
            directly_awaited: false,
            awaited_value: None,
            awaited_arity: 0,
            async_closures: vec![],
        };
        let expected = index
            .select(&expected_path, &expected_selector)
            .expect("expected implementation");
        match index.resolve_substituted_field(function, &call, substitution) {
            Some(Target::Candidate(target, reason)) => {
                assert_eq!(target, expected, "resolved candidate");
                assert!(reason.contains("via impl"), "candidate provenance");
            }
            other => panic!("no unique candidate: {other:?}"),
        }
    }
}
