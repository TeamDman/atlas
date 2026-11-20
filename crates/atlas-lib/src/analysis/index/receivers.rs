//! Written struct fields and inherent method sites, without type inference.
use super::{Index, mounts::Mounts};
use crate::{
    Result,
    analysis::{
        sources::Sources,
        workflow::{Site, facts},
    },
    source::{
        model::SourceGraph,
        population::{SyntaxSelection, TestSelection},
    },
};
use std::collections::{BTreeMap, BTreeSet};
use syn::{Item, spanned::Spanned};

#[derive(Clone)]
pub(super) struct StructShape {
    pub scope: Vec<String>,
    pub imports: BTreeMap<String, Vec<Vec<String>>>,
    pub fields: BTreeMap<String, Vec<String>>,
    pub optional_fields: BTreeMap<String, OptionalField>,
    pub ambiguous_fields: BTreeSet<String>,
    /// One written generic parameter, with optional lifetimes but no consts.
    pub generic_parameter: Option<String>,
    pub supported: bool,
    pub public: bool,
}
#[derive(Clone)]
pub(super) struct OptionalField {
    pub payload: Vec<String>,
    pub option_site: Site,
}
#[derive(Clone)]
pub(super) struct EnumShape {
    pub scope: Vec<String>,
    pub imports: BTreeMap<String, Vec<Vec<String>>>,
    pub variants: BTreeMap<String, Vec<Vec<String>>>,
    pub ambiguous_variants: BTreeSet<String>,
    pub public: bool,
    pub supported: bool,
}

#[derive(Clone)]
pub(super) struct TraitShape {
    pub methods: BTreeMap<String, Vec<Option<Site>>>,
    pub supported: bool,
    pub associated_types: BTreeSet<String>,
}

#[derive(Clone)]
pub(super) struct TraitImpl {
    pub scope: Vec<String>,
    pub imports: BTreeMap<String, Vec<Vec<String>>>,
    pub trait_path: Vec<String>,
    pub owner: Vec<String>,
    pub generic_owner: bool,
    pub supported: bool,
    pub methods: BTreeMap<String, Vec<Site>>,
    pub site: Site,
    /// One type parameter used directly as the sole type argument of the owner.
    pub future_parameter: Option<String>,
    pub future_bounds: Option<FutureBounds>,
    pub future_method_ok: bool,
    pub associated_types: BTreeSet<String>,
}

#[derive(Clone, Default)]
pub(super) struct FutureBounds {
    pub traits: Vec<Vec<String>>,
    pub associated_types: BTreeSet<String>,
}

/// Scope-relative observations reusable when one physical file has several mounts.
#[derive(Default)]
pub(in crate::analysis) struct ReceiverFacts {
    structs: BTreeMap<Vec<String>, Vec<StructShape>>,
    enums: BTreeMap<Vec<String>, Vec<EnumShape>>,
    inherent: BTreeSet<Site>,
    generic_inherent: BTreeSet<Site>,
    generic_owner_inherent: BTreeSet<Site>,
    conditional_inherent: BTreeMap<Site, String>,
    traits: BTreeMap<Vec<String>, Vec<TraitShape>>,
    impls: Vec<TraitImpl>,
}

pub(in crate::analysis) fn read(
    source: &crate::source::Source,
    file: &str,
    ast: &syn::File,
    tests: TestSelection,
) -> ReceiverFacts {
    let mut facts = ReceiverFacts::default();
    collect(
        &ast.items,
        &[],
        SyntaxSelection::Selected { source, tests },
        file,
        source.text(),
        &mut facts.structs,
        &mut facts.enums,
        &mut facts.inherent,
        &mut facts.generic_inherent,
        &mut facts.generic_owner_inherent,
        &mut facts.conditional_inherent,
        &mut facts.traits,
        &mut facts.impls,
    );
    facts
}

fn conditional(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| a.path().is_ident("cfg") || a.path().is_ident("cfg_attr"))
}

fn future_parameter(item: &syn::ItemImpl, owner: &syn::TypePath) -> Option<String> {
    if conditional(&item.attrs)
        || item.modifiers.polarity.is_some()
        || item.modifiers.defaultness.is_some()
        || item.generics.const_params().next().is_some()
        || item.trait_.as_ref().is_none_or(|(path, _)| {
            path.segments
                .iter()
                .any(|segment| !matches!(segment.arguments, syn::PathArguments::None))
        })
    {
        return None;
    }
    let params: Vec<_> = item.generics.type_params().collect();
    let [parameter] = params.as_slice() else {
        return None;
    };
    let last = owner.path.segments.last()?;
    if owner
        .path
        .segments
        .iter()
        .take(owner.path.segments.len().saturating_sub(1))
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
            syn::GenericArgument::Type(syn::Type::Path(path)) if path.qself.is_none() => {
                types.push(&path.path);
            }
            syn::GenericArgument::Lifetime(_) => {}
            _ => return None,
        }
    }
    let [argument] = types.as_slice() else {
        return None;
    };
    (argument.is_ident(&parameter.ident)).then(|| parameter.ident.to_string())
}

/// Accept only simple trait bounds on the type parameter and lifetime bounds
/// on it or its associated types. Other predicates remain unresolved.
fn future_bounds(item: &syn::ItemImpl, parameter: &str) -> Option<FutureBounds> {
    let mut traits = Vec::new();
    let mut associated_types = BTreeSet::new();
    let mut collect_bounds =
        |bounds: &syn::punctuated::Punctuated<syn::TypeParamBound, syn::Token![+]>,
         associated: bool|
         -> Option<()> {
            for bound in bounds {
                match bound {
                    syn::TypeParamBound::Lifetime(_) => {}
                    syn::TypeParamBound::Trait(t)
                        if !associated
                            && t.maybe.is_none()
                            && t.lifetimes.is_none()
                            && t.modifiers.require_empty().is_ok()
                            && t.path
                                .segments
                                .iter()
                                .all(|s| matches!(s.arguments, syn::PathArguments::None)) =>
                    {
                        traits.push(
                            t.path
                                .segments
                                .iter()
                                .map(|s| s.ident.to_string())
                                .collect(),
                        );
                    }
                    _ => return None,
                }
            }
            Some(())
        };
    for lifetime in item.generics.lifetimes() {
        if !lifetime.bounds.is_empty() {
            return None;
        }
    }
    for param in item.generics.type_params() {
        if param.ident != parameter {
            return None;
        }
        collect_bounds(&param.bounds, false)?;
    }
    if let Some(where_clause) = &item.generics.where_clause {
        for predicate in &where_clause.predicates {
            let syn::WherePredicate::Type(predicate) = predicate else {
                return None;
            };
            if predicate.lifetimes.is_some() || !predicate.attrs.is_empty() {
                return None;
            }
            let syn::Type::Path(path) = &predicate.bounded_ty else {
                return None;
            };
            if path.qself.is_some()
                || path
                    .path
                    .segments
                    .iter()
                    .any(|s| !matches!(s.arguments, syn::PathArguments::None))
            {
                return None;
            }
            let names: Vec<_> = path
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            let associated = match names.as_slice() {
                [name] if name == parameter => false,
                [name, associated] if name == parameter => {
                    associated_types.insert(associated.clone());
                    true
                }
                _ => return None,
            };
            collect_bounds(&predicate.bounds, associated)?;
        }
    }
    Some(FutureBounds {
        traits,
        associated_types,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "The source walk fills distinct indexes for the selected file and scope"
)]
fn collect(
    items: &[Item],
    scope: &[String],
    selection: SyntaxSelection<'_>,
    file: &str,
    source: &str,
    structs: &mut BTreeMap<Vec<String>, Vec<StructShape>>,
    enums: &mut BTreeMap<Vec<String>, Vec<EnumShape>>,
    inherent: &mut BTreeSet<Site>,
    generic_inherent: &mut BTreeSet<Site>,
    generic_owner_inherent: &mut BTreeSet<Site>,
    conditional_inherent: &mut BTreeMap<Site, String>,
    traits: &mut BTreeMap<Vec<String>, Vec<TraitShape>>,
    impls: &mut Vec<TraitImpl>,
) {
    let imports = facts::imports(items, selection);
    for item in items {
        if !selection.admits(item.span()) {
            continue;
        }
        match item {
            Item::Mod(m) if m.content.is_some() && !conditional(&m.attrs) => {
                let mut child = scope.to_vec();
                child.push(m.ident.to_string());
                if let Some((_, nested)) = &m.content {
                    collect(
                        nested,
                        &child,
                        selection,
                        file,
                        source,
                        structs,
                        enums,
                        inherent,
                        generic_inherent,
                        generic_owner_inherent,
                        conditional_inherent,
                        traits,
                        impls,
                    );
                }
            }
            Item::Enum(item) => {
                let mut key = scope.to_vec();
                key.push(item.ident.to_string());
                let mut variants = BTreeMap::<String, Vec<Vec<String>>>::new();
                let mut ambiguous_variants = BTreeSet::new();
                let mut seen_variants = BTreeSet::new();
                for variant in &item.variants {
                    let name = variant.ident.to_string();
                    if !seen_variants.insert(name.clone()) {
                        ambiguous_variants.insert(name.clone());
                    }
                    if conditional(&variant.attrs) {
                        ambiguous_variants.insert(name);
                        continue;
                    }
                    let syn::Fields::Unnamed(fields) = &variant.fields else {
                        continue;
                    };
                    if fields.unnamed.len() != 1 {
                        continue;
                    }
                    let Some(field) = fields.unnamed.first() else {
                        continue;
                    };
                    if conditional(&field.attrs) {
                        ambiguous_variants.insert(name);
                        continue;
                    }
                    let syn::Type::Path(payload) = &field.ty else {
                        continue;
                    };
                    if payload.qself.is_some()
                        || payload
                            .path
                            .segments
                            .iter()
                            .any(|part| !matches!(part.arguments, syn::PathArguments::None))
                    {
                        continue;
                    }
                    variants.entry(name).or_default().push(
                        payload
                            .path
                            .segments
                            .iter()
                            .map(|part| part.ident.to_string())
                            .collect(),
                    );
                }
                enums.entry(key).or_default().push(EnumShape {
                    scope: scope.to_vec(),
                    imports: imports.clone(),
                    variants,
                    ambiguous_variants,
                    public: matches!(item.vis, syn::Visibility::Public(_)),
                    supported: item.generics.params.is_empty()
                        && item.generics.where_clause.is_none()
                        && !conditional(&item.attrs),
                });
            }
            Item::Struct(s) => {
                let mut fields = BTreeMap::new();
                let mut optional_fields = BTreeMap::new();
                let mut ambiguous_fields = BTreeSet::new();
                for field in &s.fields {
                    if conditional(&field.attrs) {
                        if let Some(name) = &field.ident {
                            ambiguous_fields.insert(name.to_string());
                        }
                        continue;
                    }
                    let (Some(name), syn::Type::Path(ty)) = (&field.ident, &field.ty) else {
                        continue;
                    };
                    if ty.qself.is_none()
                        && ty.path.segments.len() == 1
                        && let Some(segment) = ty.path.segments.first()
                        && segment.ident == "Option"
                        && let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments
                        && let [syn::GenericArgument::Type(syn::Type::Path(payload))] =
                            arguments.args.iter().collect::<Vec<_>>().as_slice()
                        && payload.qself.is_none()
                        && payload
                            .path
                            .segments
                            .iter()
                            .all(|part| matches!(part.arguments, syn::PathArguments::None))
                    {
                        let entry = OptionalField {
                            payload: payload
                                .path
                                .segments
                                .iter()
                                .map(|part| part.ident.to_string())
                                .collect(),
                            option_site: Site::at(file, segment.ident.span()),
                        };
                        if optional_fields.insert(name.to_string(), entry).is_some() {
                            ambiguous_fields.insert(name.to_string());
                        }
                        continue;
                    }
                    if ty.qself.is_some()
                        || ty
                            .path
                            .segments
                            .iter()
                            .any(|s| !matches!(s.arguments, syn::PathArguments::None))
                    {
                        continue;
                    }
                    if fields
                        .insert(
                            name.to_string(),
                            ty.path
                                .segments
                                .iter()
                                .map(|s| s.ident.to_string())
                                .collect(),
                        )
                        .is_some()
                    {
                        ambiguous_fields.insert(name.to_string());
                    }
                }
                let type_parameters: Vec<_> = s.generics.type_params().collect();
                let generic_parameter = match type_parameters.as_slice() {
                    [parameter]
                        if !conditional(&s.attrs)
                            && s.generics.const_params().next().is_none()
                            && parameter.default.is_none() =>
                    {
                        Some(parameter.ident.to_string())
                    }
                    _ => None,
                };
                let mut key = scope.to_vec();
                key.push(s.ident.to_string());
                structs.entry(key).or_default().push(StructShape {
                    scope: scope.to_vec(),
                    imports: imports.clone(),
                    fields,
                    optional_fields,
                    ambiguous_fields,
                    generic_parameter,
                    supported: s.generics.params.is_empty()
                        && s.generics.where_clause.is_none()
                        && !conditional(&s.attrs),
                    public: matches!(s.vis, syn::Visibility::Public(_)),
                });
            }
            Item::Impl(i)
                if i.trait_.is_none()
                    && i.generics.params.is_empty()
                    && i.generics.where_clause.is_none()
                    && !conditional(&i.attrs) =>
            {
                for member in &i.items {
                    if let syn::ImplItem::Fn(f) = member
                        && selection.admits(f.span())
                        && f.sig.receiver().is_some()
                    {
                        if conditional(&f.attrs) {
                            let gates = f
                                .attrs
                                .iter()
                                .filter(|a| {
                                    a.path().is_ident("cfg") || a.path().is_ident("cfg_attr")
                                })
                                .map(|a| crate::source_text::text(source, a.span()))
                                .collect::<Vec<_>>()
                                .join(" ");
                            conditional_inherent.insert(Site::at(file, f.sig.span()), gates);
                            continue;
                        }
                        let sites = if f.sig.generics.type_params().next().is_none() {
                            &mut *inherent
                        } else {
                            &mut *generic_inherent
                        };
                        sites.insert(Site::at(file, f.sig.span()));
                    }
                }
            }
            Item::Impl(i)
                if i.trait_.is_none()
                    && !conditional(&i.attrs)
                    && i.generics.const_params().next().is_none() =>
            {
                for member in &i.items {
                    if let syn::ImplItem::Fn(f) = member
                        && selection.admits(f.span())
                        && !conditional(&f.attrs)
                        && f.sig.receiver().is_some()
                    {
                        generic_owner_inherent.insert(Site::at(file, f.sig.span()));
                    }
                }
            }
            Item::Trait(t) => {
                let mut methods: BTreeMap<String, Vec<Option<Site>>> = BTreeMap::new();
                let mut associated_types = BTreeSet::new();
                let mut supported = t.generics.params.is_empty()
                    && t.generics.where_clause.is_none()
                    && !conditional(&t.attrs);
                for member in &t.items {
                    if let syn::TraitItem::Type(ty) = member {
                        if conditional(&ty.attrs)
                            || !ty.generics.params.is_empty()
                            || ty.generics.where_clause.is_some()
                        {
                            supported = false;
                        } else {
                            associated_types.insert(ty.ident.to_string());
                        }
                    }
                    if let syn::TraitItem::Fn(f) = member
                        && selection.admits(f.span())
                    {
                        supported &= !conditional(&f.attrs)
                            && f.sig.receiver().is_some()
                            && f.sig.generics.type_params().next().is_none();
                        methods
                            .entry(f.sig.ident.to_string())
                            .or_default()
                            .push(f.default.as_ref().map(|_| Site::at(file, f.sig.span())));
                    }
                }
                let mut key = scope.to_vec();
                key.push(t.ident.to_string());
                traits.entry(key).or_default().push(TraitShape {
                    methods,
                    supported,
                    associated_types,
                });
            }
            Item::Impl(i) if i.trait_.is_some() => {
                let Some((trait_path, _)) = &i.trait_ else {
                    continue;
                };
                let syn::Type::Path(owner) = &*i.self_ty else {
                    continue;
                };
                if owner.qself.is_some() {
                    continue;
                }
                let owner_parts: Vec<_> = owner
                    .path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect();
                let generic_owner = owner_parts
                    .first()
                    .is_some_and(|head| i.generics.type_params().any(|p| p.ident == head));
                let mut supported = i.generics.params.is_empty()
                    && i.generics.where_clause.is_none()
                    && !conditional(&i.attrs)
                    && owner
                        .path
                        .segments
                        .iter()
                        .all(|s| matches!(s.arguments, syn::PathArguments::None))
                    && trait_path
                        .segments
                        .iter()
                        .all(|s| matches!(s.arguments, syn::PathArguments::None));
                let mut methods: BTreeMap<String, Vec<Site>> = BTreeMap::new();
                let mut future_method_ok = true;
                let mut associated_types = BTreeSet::new();
                for member in &i.items {
                    if let syn::ImplItem::Type(ty) = member {
                        if conditional(&ty.attrs)
                            || !ty.generics.params.is_empty()
                            || ty.generics.where_clause.is_some()
                        {
                            supported = false;
                        } else {
                            associated_types.insert(ty.ident.to_string());
                        }
                    }
                    if let syn::ImplItem::Fn(f) = member
                        && selection.admits(f.span())
                    {
                        if f.sig.ident == "into_future" {
                            future_method_ok &= !conditional(&f.attrs)
                                && f.sig.asyncness.is_none()
                                && f.sig.receiver().is_some_and(|receiver| {
                                    matches!(receiver.kind, syn::ReceiverKind::Value)
                                })
                                && f.sig.generics.type_params().next().is_none();
                        }
                        supported &= !conditional(&f.attrs)
                            && f.sig.receiver().is_some()
                            && f.sig.generics.type_params().next().is_none();
                        methods
                            .entry(f.sig.ident.to_string())
                            .or_default()
                            .push(Site::at(file, f.sig.span()));
                    }
                }
                let future_parameter = future_parameter(i, owner);
                let future_bounds = future_parameter
                    .as_deref()
                    .and_then(|parameter| future_bounds(i, parameter));
                impls.push(TraitImpl {
                    scope: scope.to_vec(),
                    imports: imports.clone(),
                    trait_path: trait_path
                        .segments
                        .iter()
                        .map(|s| s.ident.to_string())
                        .collect(),
                    owner: owner_parts,
                    generic_owner,
                    supported,
                    methods,
                    site: Site::at(file, i.span()),
                    future_parameter,
                    future_bounds,
                    future_method_ok,
                    associated_types,
                });
            }
            _ => {}
        }
    }
}

impl Index {
    pub(super) fn read_structs(
        &mut self,
        sources: &Sources,
        graph: &SourceGraph,
        mounts: &Mounts,
        tests: TestSelection,
    ) -> Result<()> {
        for file in graph
            .physical_files()
            .filter(|f| tests.admits_file(f.test_only))
        {
            let Some(contexts) = mounts.files.get(&file.path) else {
                continue;
            };
            let source = &sources[&file.path];
            let facts = source.receivers(&file.path, tests)?;
            for (scope, _) in contexts {
                for (path, shapes) in &facts.structs {
                    let key = scope.iter().chain(path).cloned().collect();
                    let shapes = shapes.iter().cloned().map(|mut shape| {
                        shape.scope = scope.iter().chain(&shape.scope).cloned().collect();
                        shape
                    });
                    self.structs.entry(key).or_default().extend(shapes);
                }
                for (path, shapes) in &facts.enums {
                    let key = scope.iter().chain(path).cloned().collect();
                    let shapes = shapes.iter().cloned().map(|mut shape| {
                        shape.scope = scope.iter().chain(&shape.scope).cloned().collect();
                        shape
                    });
                    self.enums.entry(key).or_default().extend(shapes);
                }
                self.inherent_sites.extend(facts.inherent.iter().cloned());
                self.generic_inherent_sites
                    .extend(facts.generic_inherent.iter().cloned());
                self.generic_owner_inherent_sites
                    .extend(facts.generic_owner_inherent.iter().cloned());
                self.conditional_inherent_sites
                    .extend(facts.conditional_inherent.clone());
                for (path, shapes) in &facts.traits {
                    let key = scope.iter().chain(path).cloned().collect();
                    self.traits
                        .entry(key)
                        .or_default()
                        .extend(shapes.iter().cloned());
                }
                self.impls
                    .extend(facts.impls.iter().cloned().map(|mut implementation| {
                        implementation.scope =
                            scope.iter().chain(&implementation.scope).cloned().collect();
                        implementation
                    }));
            }
        }
        Ok(())
    }
}
