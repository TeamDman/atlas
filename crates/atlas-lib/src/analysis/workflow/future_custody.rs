//! Bounded written value custody under externally declared wrapper contracts.
//! This is a source candidate, not Rust type inference or an execution trace.
use super::{
    Call, DeferredLocal, Site, Workflow,
    future_profile::{FutureProfile, Operation},
};
use std::collections::BTreeMap;
use syn::Expr;

#[derive(Clone)]
pub(super) struct FutureValue {
    pub future: DeferredLocal,
    pub origin: usize,
    pub callbacks: BTreeMap<String, std::rc::Rc<super::projection::CallbackBinding>>,
}

fn before(a: &Site, b: &Site) -> bool {
    a.path == b.path && (a.end_line, a.end_column) <= (b.line, b.column)
}
fn wrapper<'a>(
    flow: &'a Workflow,
    profile: &FutureProfile,
    within: &Site,
    name: &str,
    operation: Operation,
) -> Option<&'a Call> {
    let calls: Vec<_> = flow
        .calls
        .iter()
        .filter(|call| within.contains(&call.site) && call.spelling == name)
        .collect();
    let [call] = calls.as_slice() else {
        return None;
    };
    (profile.operation(&call.site, name)?.operation == operation).then_some(*call)
}

/// Follow only top-level immutable locals, using position to respect shadowing.
/// Branch-local bindings, writes, ambiguity, unprofiled wrappers and unsupported
/// expressions refuse rather than selecting an apparent value.
pub(super) fn value(
    flow: &Workflow,
    function: usize,
    profile: &FutureProfile,
    incoming: &BTreeMap<String, FutureValue>,
    expr: &Expr,
    context: &Site,
    depth: usize,
) -> Option<FutureValue> {
    if depth >= 16 {
        return None;
    }
    match expr {
        Expr::Paren(inner) => value(
            flow,
            function,
            profile,
            incoming,
            &inner.expr,
            context,
            depth + 1,
        ),
        Expr::Group(inner) => value(
            flow,
            function,
            profile,
            incoming,
            &inner.expr,
            context,
            depth + 1,
        ),
        Expr::Path(path) if path.qself.is_none() => {
            let name = path.path.get_ident()?.to_string();
            if flow.bindings.iter().filter(|b| b.name == name).any(|b| {
                !flow.site.contains(&b.site)
                    && !flow
                        .local_values
                        .iter()
                        .any(|local| local.binding == name && local.site == b.site)
            }) {
                return None;
            }
            let previous: Vec<_> = flow
                .local_values
                .iter()
                .filter(|local| local.binding == name && before(&local.site, context))
                .collect();
            let latest = previous
                .iter()
                .max_by_key(|local| (local.site.end_line, local.site.end_column));
            if let Some(local) = latest {
                if previous
                    .iter()
                    .filter(|other| other.site == local.site)
                    .count()
                    != 1
                {
                    return None;
                }
                let candidates: Vec<_> = flow
                    .deferred_locals
                    .iter()
                    .filter(|future| {
                        future.binding == name && local.site.contains(&future.initializer)
                    })
                    .collect();
                if let [future] = candidates.as_slice() {
                    let valid = future.wrappers.iter().all(|w| {
                        profile
                            .operation(&w.site, &w.spelling)
                            .is_some_and(|c| match c.operation {
                                Operation::ReceiverForward => w.is_method,
                                Operation::ArgumentForward { argument: 0 } => !w.is_method,
                                _ => false,
                            })
                    });
                    return valid.then(|| FutureValue {
                        future: (*future).clone(),
                        origin: function,
                        callbacks: BTreeMap::new(),
                    });
                }
                let init: Expr = syn::parse_str(&local.initializer).ok()?;
                return value(
                    flow,
                    function,
                    profile,
                    incoming,
                    &init,
                    &local.site,
                    depth + 1,
                );
            }
            incoming.get(&name).cloned()
        }
        Expr::MethodCall(call) if call.attrs.is_empty() => {
            let name = call.method.to_string();
            let observed = wrapper(flow, profile, context, &name, Operation::ReceiverForward)?;
            value(
                flow,
                function,
                profile,
                incoming,
                &call.receiver,
                &observed.site,
                depth + 1,
            )
        }
        Expr::Call(call) if call.attrs.is_empty() => {
            let Expr::Path(path) = &*call.func else {
                return None;
            };
            if path.qself.is_some() {
                return None;
            }
            let name = path
                .path
                .segments
                .iter()
                .map(|part| part.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");
            let observed = wrapper(
                flow,
                profile,
                context,
                &name,
                Operation::ArgumentForward { argument: 0 },
            )?;
            if call.args.len() != 1 {
                return None;
            }
            value(
                flow,
                function,
                profile,
                incoming,
                call.args.first()?,
                &observed.site,
                depth + 1,
            )
        }
        _ => None,
    }
}

pub(super) fn arguments(
    caller: &Workflow,
    caller_index: usize,
    target: &Workflow,
    call: &Call,
    profile: &FutureProfile,
    incoming: &BTreeMap<String, FutureValue>,
) -> BTreeMap<String, FutureValue> {
    let parameters: Vec<_> = target
        .bindings
        .iter()
        .filter(|b| target.site.contains(&b.site))
        .collect();
    if parameters.len() != call.arguments.len() {
        return BTreeMap::new();
    }
    parameters
        .iter()
        .zip(&call.arguments)
        .filter_map(|(parameter, text)| {
            let expr: Expr = syn::parse_str(text).ok()?;
            value(
                caller,
                caller_index,
                profile,
                incoming,
                &expr,
                &call.site,
                0,
            )
            .map(|future| (parameter.name.clone(), future))
        })
        .collect()
}
