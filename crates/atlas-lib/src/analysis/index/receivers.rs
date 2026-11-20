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
    pub supported: bool,
    pub public: bool,
}

#[derive(Clone)]
pub(super) struct TraitShape {
    pub methods: BTreeMap<String, Vec<Option<Site>>>,
    pub supported: bool,
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
}

/// Scope-relative observations reusable when one physical file has several mounts.
#[derive(Default)]
pub(in crate::analysis) struct ReceiverFacts {
    structs: BTreeMap<Vec<String>, Vec<StructShape>>,
    inherent: BTreeSet<Site>,
    generic_inherent: BTreeSet<Site>,
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
        &mut facts.structs,
        &mut facts.inherent,
        &mut facts.generic_inherent,
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

#[expect(
    clippy::too_many_arguments,
    reason = "The source walk fills distinct indexes for the selected file and scope"
)]
fn collect(
    items: &[Item],
    scope: &[String],
    selection: SyntaxSelection<'_>,
    file: &str,
    structs: &mut BTreeMap<Vec<String>, Vec<StructShape>>,
    inherent: &mut BTreeSet<Site>,
    generic_inherent: &mut BTreeSet<Site>,
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
                        structs,
                        inherent,
                        generic_inherent,
                        traits,
                        impls,
                    );
                }
            }
            Item::Struct(s) => {
                let mut fields = BTreeMap::new();
                for field in &s.fields {
                    if conditional(&field.attrs) {
                        continue;
                    }
                    let (Some(name), syn::Type::Path(ty)) = (&field.ident, &field.ty) else {
                        continue;
                    };
                    if ty.qself.is_some()
                        || ty
                            .path
                            .segments
                            .iter()
                            .any(|s| !matches!(s.arguments, syn::PathArguments::None))
                    {
                        continue;
                    }
                    fields.insert(
                        name.to_string(),
                        ty.path
                            .segments
                            .iter()
                            .map(|s| s.ident.to_string())
                            .collect(),
                    );
                }
                let mut key = scope.to_vec();
                key.push(s.ident.to_string());
                structs.entry(key).or_default().push(StructShape {
                    scope: scope.to_vec(),
                    imports: imports.clone(),
                    fields,
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
                        && !conditional(&f.attrs)
                        && f.sig.receiver().is_some()
                    {
                        let sites = if f.sig.generics.type_params().next().is_none() {
                            &mut *inherent
                        } else {
                            &mut *generic_inherent
                        };
                        sites.insert(Site::at(file, f.sig.span()));
                    }
                }
            }
            Item::Trait(t) => {
                let mut methods: BTreeMap<String, Vec<Option<Site>>> = BTreeMap::new();
                let mut supported = t.generics.params.is_empty()
                    && t.generics.where_clause.is_none()
                    && !conditional(&t.attrs);
                for member in &t.items {
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
                traits
                    .entry(key)
                    .or_default()
                    .push(TraitShape { methods, supported });
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
                for member in &i.items {
                    if let syn::ImplItem::Fn(f) = member
                        && selection.admits(f.span())
                    {
                        supported &= !conditional(&f.attrs)
                            && f.sig.receiver().is_some()
                            && f.sig.generics.type_params().next().is_none();
                        methods
                            .entry(f.sig.ident.to_string())
                            .or_default()
                            .push(Site::at(file, f.sig.span()));
                    }
                }
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
                self.inherent_sites.extend(facts.inherent.iter().cloned());
                self.generic_inherent_sites
                    .extend(facts.generic_inherent.iter().cloned());
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
