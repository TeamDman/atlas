//! Source control and matched call occurrences, grouped by physical file.
use super::{Detail, ExitKind, LoopKind, Site, Step, Workflow, compact::Folds};
use crate::{
    Result,
    analysis::index::{AwaitSubstitution, Index, Target},
    diagram::{Coverage, Diagram, Edge, EdgeEvidence, Level, Node, View},
};
use std::{fmt::Write as _, rc::Rc};

pub(super) fn diagram(flow: &Workflow) -> Diagram {
    let mut b = Builder::new(None, usize::MAX, Detail::Expressions);
    b.function(flow, None, 0);
    b.finish(flow)
}
pub(super) fn with_detail(flow: &Workflow, detail: Detail) -> Result<Diagram> {
    let budget = if detail == Detail::Control {
        600
    } else {
        usize::MAX
    };
    let mut b = Builder::new(None, budget, detail);
    b.function(flow, None, 0);
    b.checked(flow)
}
pub(super) fn expanded(index: &Index, entry: usize, depth: u8, detail: Detail) -> Result<Diagram> {
    expanded_with_profile(index, entry, depth, detail, None)
}
pub(super) fn expanded_with_profile(
    index: &Index,
    entry: usize,
    depth: u8,
    detail: Detail,
    profile: Option<&super::future_profile::FutureProfile>,
) -> Result<Diagram> {
    let flow = &index
        .functions()
        .get(entry)
        .ok_or_else(|| format!("invalid indexed function position {entry}"))?
        .flow;
    let mut b = Builder::new(Some(index), 600, detail);
    b.future_profile = profile;
    if let Some(profile) = profile {
        if !profile.scope.is_empty() {
            b.scope = Some(
                profile
                    .scope
                    .iter()
                    .map(|selector| index.select(&selector.path, &selector.function))
                    .collect::<Result<_>>()?,
            );
        }
        b.declared.push("Future-wrapper contracts and call-token resolutions below are supplied input, not compiler proof or an execution trace; execution remains conditional on reaching and polling the value.".into());
        b.declared.extend(profile.scope.iter().map(|selector| {
            format!(
                "Profile expansion scope {} :: {}",
                selector.path, selector.function
            )
        }));
        b.declared.extend(profile.contracts.iter().map(|contract| {
            format!(
                "Declared future contract {}: {}",
                contract.definition, contract.evidence
            )
        }));
        b.declared
            .extend(profile.resolutions.iter().map(|resolution| {
                format!(
                    "Declared resolution {}:{}:{} {} -> {}",
                    resolution.path,
                    resolution.line,
                    resolution.column,
                    resolution.spelling,
                    resolution.definition
                )
            }));
        b.declared.extend(profile.sources.iter().map(|source| {
            format!(
                "Future profile source {} sha256 {}",
                source.path, source.sha256
            )
        }));
    }
    b.function(flow, Some(entry), depth);
    b.checked(flow)
}
struct Builder<'a> {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    group: String,
    loops: Vec<LoopTarget>,
    index: Option<&'a Index>,
    stack: Vec<usize>,
    function: Option<usize>,
    exit: String,
    depth: u8,
    budget: usize,
    exhausted: bool,
    folds: Option<Folds>,
    targets: std::collections::BTreeMap<(usize, Site), Target>,
    /// One note per body-local declaration of every drawn body, once each.
    declared: Vec<String>,
    substitution: Option<AwaitSubstitution>,
    callbacks: std::collections::BTreeMap<String, Rc<CallbackBinding>>,
    deferred_calls: Option<Vec<super::Call>>,
    future_profile: Option<&'a super::future_profile::FutureProfile>,
    future_values: std::collections::BTreeMap<String, super::future_custody::FutureValue>,
    scope: Option<std::collections::BTreeSet<usize>>,
    guarded_factories: Vec<super::GuardedFactory>,
    deferred_receivers: std::collections::BTreeMap<String, Vec<String>>,
}
pub(super) struct CallbackBinding {
    closure: super::AsyncClosure,
    origin: usize,
    captured: std::collections::BTreeMap<String, Rc<Self>>,
    receiver_types: std::collections::BTreeMap<String, Vec<String>>,
}
struct LoopTarget {
    name: Option<String>,
    header: String,
    exit: String,
    /// A `while` condition: only a labelled transfer may name this loop, and
    /// an unlabelled one there is not valid Rust, so it reaches no target.
    labelled_only: bool,
    /// A labelled block: only `break 'label` leaves it; it has no `continue`.
    block: bool,
}
impl<'a> Builder<'a> {
    fn new(index: Option<&'a Index>, budget: usize, detail: Detail) -> Self {
        Self {
            nodes: vec![],
            edges: vec![],
            group: String::new(),
            loops: vec![],
            index,
            stack: vec![],
            function: None,
            exit: String::new(),
            depth: 0,
            budget,
            exhausted: false,
            folds: (detail == Detail::Control).then(Folds::default),
            targets: std::collections::BTreeMap::new(),
            declared: Vec::new(),
            substitution: None,
            callbacks: std::collections::BTreeMap::new(),
            deferred_calls: None,
            future_profile: None,
            future_values: std::collections::BTreeMap::new(),
            scope: None,
            guarded_factories: Vec::new(),
            deferred_receivers: std::collections::BTreeMap::new(),
        }
    }
    fn checked(self, flow: &Workflow) -> Result<Diagram> {
        if let Some(position) = self.targets.values().find_map(|target| match target {
            Target::InvalidFunction(position) => Some(position),
            _ => None,
        }) {
            return Err(format!("invalid indexed function position {position}").into());
        }
        if self.exhausted {
            return Err("flow exceeds 600 source-control nodes; lower --depth or select a smaller function (nothing emitted)".into());
        }
        Ok(self.finish(flow))
    }
    fn finish(self, flow: &Workflow) -> Diagram {
        let files: std::collections::BTreeSet<_> =
            self.nodes.iter().map(|n| n.group.clone()).collect();
        let diagram = Diagram {
            title: format!("{} — source control and call occurrences", flow.function),
            level: Level::Declarations,
            view: View::Workflow,
            nodes: self.nodes,
            edges: self.edges,
            inventory: files.iter().cloned().collect(),
            coverage: Coverage {
                represented_files: files.into_iter().collect(),
                ..Coverage::default()
            },
            notes: vec![
                "Source-control possibilities, not path feasibility or a proof of termination. Panics, unwinding, destructors, overloaded operations and macro effects are not inferred.".into(),
                "Call occurrences have distinct continuations. Unexpanded calls continue only if they return; recursion and unresolved bodies are visible boundaries.".into(),
                "Pattern failure skips its guard; guard failure tries the next pattern. A ? exits its enclosing function or directly awaited async block on its residual branch.".into(),
            ]
            .into_iter()
            .chain(self.declared)
            .collect(),
            evidence_links: vec![],
        };
        if let Some(folds) = self.folds {
            folds.apply(diagram)
        } else {
            diagram
        }
    }
    fn node(&mut self, label: &str, site: &Site) -> String {
        if self.nodes.len() >= self.budget {
            self.exhausted = true;
        }
        let id = format!("flow-{}", self.nodes.len());
        self.nodes.push(Node {
            id: id.clone(),
            label: label.into(),
            group: self.group.clone(),
            source_sites: vec![site.link()],
            context: false,
            internal_relationships: vec![],
            surface: None,
        });
        id
    }
    fn connect(&mut self, from: Vec<String>, to: &str) {
        for from in from {
            self.edge(&from, to, None);
        }
    }
    fn edge(&mut self, from: &str, to: &str, label: Option<&str>) {
        self.edges.push(Edge {
            from: from.into(),
            to: to.into(),
            label: label.map(str::to_owned),
            evidence: EdgeEvidence::SourceOrder,
            relationships: vec![],
            declarations: vec![],
        });
    }
    fn function(
        &mut self,
        flow: &Workflow,
        function: Option<usize>,
        depth: u8,
    ) -> (String, String) {
        let old_group = std::mem::replace(&mut self.group, flow.site.path.clone());
        let old_loops = std::mem::take(&mut self.loops);
        let old_function = std::mem::replace(&mut self.function, function);
        let old_depth = std::mem::replace(&mut self.depth, depth);
        if let Some(i) = function {
            if let Some(index) = self.index {
                for call in &flow.calls {
                    self.targets
                        .entry((i, call.site.clone()))
                        .or_insert_with(|| index.resolve(i, call));
                }
            }
            self.stack.push(i);
        }
        if let Some(folds) = &mut self.folds {
            folds
                .call_sites
                .extend(flow.calls.iter().map(|c| c.site.clone()));
        }
        for declared in &flow.declarations {
            let note = format!(
                "{} declares {} at {}; it runs nothing, so it is not a step.",
                flow.function,
                declared.label,
                declared.site.link()
            );
            if !self.declared.contains(&note) {
                self.declared.push(note);
            }
        }
        let entry_label = self.substitution.as_ref().map_or_else(
            || format!("fn {}", flow.function),
            |substitution| {
                format!(
                    "fn {}\n[source substitution {} = {}; not compiler proof]",
                    flow.function,
                    substitution.parameter,
                    substitution.concrete.join("::")
                )
            },
        );
        let entry = self.node(&entry_label, &flow.site);
        let end = self.node(
            &format!("return from {} → {}", flow.function, flow.output),
            &flow.site,
        );
        let old_exit = std::mem::replace(&mut self.exit, end.clone());
        let tails = self.steps(&flow.steps, vec![entry.clone()]);
        self.connect(tails, &end);
        self.exit = old_exit;
        self.depth = old_depth;
        self.function = old_function;
        self.loops = old_loops;
        self.group = old_group;
        if function.is_some() {
            self.stack.pop();
        }
        (entry, end)
    }
    fn call(&mut self, id: &str, site: &Site) -> Option<String> {
        let index = self.index?;
        let f = self.function?;
        let source_call = self
            .deferred_calls
            .as_deref()
            .unwrap_or(&index.functions()[f].flow.calls)
            .iter()
            .find(|call| &call.site == site)
            .cloned();
        let source_call = source_call.as_ref();
        if let Some(profile) = self.future_profile
            && let Some(factory) = self
                .guarded_factories
                .iter()
                .find(|factory| factory.await_site == *site)
                .cloned()
        {
            let expected = factory.cases.len();
            let cases: Vec<_> = factory.cases.into_iter().filter(|case| {
                profile.operation(&case.call_site, &case.spelling).is_some_and(|contract| matches!(contract.operation, super::future_profile::Operation::GuardedFactory { argument: 0, arity } if arity == case.closure.parameters.len()))
            }).collect();
            if cases.len() == expected && expected > 0 {
                let end = self.node("guarded factory future completes (value or residual)", site);
                let unpolled = self.node("scope future not polled or interrupted [opaque]", site);
                self.edge(id, &unpolled, Some("if not polled or interrupted"));
                for case in cases {
                    let admitted = self.node(
                        "scope factory admitted and invoked [declared profile]",
                        &case.call_site,
                    );
                    self.edge(
                        id,
                        &admitted,
                        Some(&format!("{}; if polled and admitted", case.condition)),
                    );
                    let rejected = self.node("scope not admitted [opaque]", &case.call_site);
                    self.edge(
                        id,
                        &rejected,
                        Some(&format!(
                            "{}; if polled and admission rejected",
                            case.condition
                        )),
                    );
                    let poll = self.node(
                        "poll admitted factory future [declared profile]",
                        &case.closure.site,
                    );
                    self.edge(&admitted, &poll, Some("if not cancelled and polled"));
                    let cancelled = self.node(
                        "cancelled before factory future poll [opaque]",
                        &case.call_site,
                    );
                    self.edge(&admitted, &cancelled, Some("if cancelled before poll"));
                    let old_exit = std::mem::replace(&mut self.exit, end.clone());
                    let old_loops = std::mem::take(&mut self.loops);
                    let old_calls = self.deferred_calls.replace(case.closure.awaited_callbacks);
                    let ends = self.steps(&case.closure.body, vec![poll]);
                    self.connect(ends, &end);
                    self.deferred_calls = old_calls;
                    self.loops = old_loops;
                    self.exit = old_exit;
                }
                return Some(end);
            }
        }
        if let Some(call) = source_call
            && let Some(name) = call.awaited_value.as_deref()
            && let Some(binding) = self.callbacks.get(name).cloned()
            && binding.closure.parameters.len() == call.awaited_arity
        {
            let closure = binding.closure.clone();
            let poll = self.node(
                "poll callback's async future [source candidate; only if invoked and polled]",
                &closure.site,
            );
            self.edge(id, &poll, Some("if invoked and polled"));
            let end = self.node(
                "callback async future completes (value or residual)",
                &closure.site,
            );
            let outer_exit = std::mem::replace(&mut self.exit, end.clone());
            let outer_loops = std::mem::take(&mut self.loops);
            let outer_group = std::mem::replace(&mut self.group, closure.site.path.clone());
            let outer_function = self.function.replace(binding.origin);
            let mut calls = closure.awaited_callbacks;
            calls.extend(closure.awaited_parameter_methods);
            let outer_calls = self.deferred_calls.replace(calls);
            let outer_receivers =
                std::mem::replace(&mut self.deferred_receivers, binding.receiver_types.clone());
            let outer_callbacks = std::mem::replace(&mut self.callbacks, binding.captured.clone());
            let ends = self.steps(&closure.body, vec![poll]);
            self.connect(ends, &end);
            self.callbacks = outer_callbacks;
            self.deferred_calls = outer_calls;
            self.deferred_receivers = outer_receivers;
            self.function = outer_function;
            self.group = outer_group;
            self.loops = outer_loops;
            self.exit = outer_exit;
            return Some(end);
        }
        // Only the captured callback awaits were indexed in this deferred body.
        // Other calls keep their opaque source representation.
        let deferred_target = source_call.and_then(|call| {
            call.receiver
                .as_ref()
                .and_then(|receiver| self.deferred_receivers.get(receiver))
                .map(|owner| index.callback_receiver_method(f, owner, call))
        });
        if self.deferred_calls.is_some() && deferred_target.is_none() {
            return None;
        }
        if let (Some(profile), Some(call)) = (self.future_profile, source_call)
            && let Some(contract) = profile.operation(&call.site, &call.spelling)
            && matches!(
                contract.operation,
                super::future_profile::Operation::ReceiverForward
                    | super::future_profile::Operation::ArgumentForward { .. }
            )
            && let Some(node) = self.nodes.iter_mut().find(|node| node.id == id)
        {
            write!(
                node.label,
                "\n[declared future-forwarding summary: {}]",
                contract.evidence
            )
            .expect("formatting into a String cannot fail");
        }
        if let (Some(profile), Some(call)) = (self.future_profile, source_call)
            && let Some(contract) = profile.operation(&call.site, &call.spelling)
            && let super::future_profile::Operation::PollArgument { argument } = contract.operation
            && let Some(argument) = call.arguments.get(argument)
            && let Ok(expr) = syn::parse_str::<syn::Expr>(argument)
            && let Some(value) = super::future_custody::value(
                &index.functions()[f].flow,
                f,
                profile,
                &self.future_values,
                &expr,
                &call.site,
                0,
            )
        {
            let poll = self.node(
                "poll forwarded future [source candidate, declared symbol profile; only if reached and polled]",
                site,
            );
            self.edge(id, &poll, Some("if runtime polls; declared profile"));
            let unresolved = self.node("future not polled or polling interrupted [opaque]", site);
            self.edge(id, &unresolved, Some("if poll is not reached"));
            let end = self.node(
                "forwarded future completes (value or residual)",
                &value.future.future,
            );
            let old_exit = std::mem::replace(&mut self.exit, end.clone());
            let old_loops = std::mem::take(&mut self.loops);
            let old_group = std::mem::replace(&mut self.group, value.future.future.path.clone());
            let old_function = self.function.replace(value.origin);
            let old_calls = self.deferred_calls.replace(Vec::new());
            let old_factories =
                std::mem::replace(&mut self.guarded_factories, value.future.guarded_factories);
            let old_callbacks = std::mem::replace(&mut self.callbacks, value.callbacks);
            let ends = self.steps(&value.future.body, vec![poll]);
            self.connect(ends, &end);
            self.callbacks = old_callbacks;
            self.guarded_factories = old_factories;
            self.deferred_calls = old_calls;
            self.function = old_function;
            self.group = old_group;
            self.loops = old_loops;
            self.exit = old_exit;
            return Some(end);
        }
        let implicit_await = index.functions()[f]
            .flow
            .calls
            .iter()
            .any(|call| &call.site == site && call.await_producer.is_some());
        let target = deferred_target.or_else(|| {
            self.substitution
                .as_ref()
                .and_then(|substitution| {
                    source_call
                        .and_then(|call| index.resolve_substituted_field(f, call, substitution))
                })
                .or_else(|| self.targets.get(&(f, site.clone())).cloned())
        })?;
        let (target, reason, candidate) = match target {
            Target::Function(j) if self.stack.contains(&j) => {
                (None, "recursion boundary".into(), false)
            }
            Target::Function(_) if self.depth == 0 => (None, "depth boundary".into(), false),
            Target::Function(j) => (Some(j), String::new(), false),
            Target::Candidate(j, _) if self.stack.contains(&j) => {
                (None, "candidate recursion boundary".into(), true)
            }
            Target::Candidate(_, reason) if self.depth == 0 => {
                (None, format!("candidate depth boundary: {reason}"), true)
            }
            Target::Candidate(j, reason) => (Some(j), reason, true),
            Target::Summarized(s) => (None, s, false),
            Target::Boundary(..) => (None, "declared boundary".into(), false),
            Target::InvalidFunction(position) => {
                (None, format!("invalid function position {position}"), false)
            }
        };
        if candidate && let Some(node) = self.nodes.iter_mut().find(|n| n.id == id) {
            write!(
                node.label,
                "\n[source candidate, not proved dispatch: {reason}]"
            )
            .expect("formatting into a String cannot fail");
        }
        let Some(j) = target else {
            if let Some(node) = self.nodes.iter_mut().find(|n| n.id == id) {
                write!(
                    node.label,
                    "\n[not expanded: {reason}; continues if it returns]"
                )
                .expect("formatting into a String cannot fail");
            }
            return Some(id.to_owned());
        };
        if let Some(scope) = &self.scope
            && !scope.contains(&j)
        {
            if let Some(node) = self.nodes.iter_mut().find(|node| node.id == id) {
                node.label
                    .push_str("\n[profile cut point: body not expanded; continues if it returns]");
            }
            return Some(id.to_owned());
        }
        if let Some(call) = source_call
            && index.callback_bound_unshadowed(j)
            && let Some(node) = self.nodes.iter_mut().find(|node| node.id == id)
        {
            for parameter in &index.functions()[j].flow.callback_parameters {
                for closure in call.async_closures.iter().filter(|closure| {
                    closure.argument == parameter.argument
                        && closure.parameters.len() == parameter.arity
                }) {
                    write!(node.label, "\n[written callback argument {} matches FnOnce parameter {} with {} arguments; closure at {}; invocation and polling not established]", closure.argument + 1, parameter.name, parameter.arity, closure.site.link()).expect("formatting into a String cannot fail");
                }
            }
        }
        let async_target = index.functions()[j].flow.async_function;
        let callback_target = !index.functions()[j].flow.callback_parameters.is_empty();
        if async_target
            && !source_call.is_some_and(|call| call.directly_awaited || call.awaited_block_call)
        {
            if let Some(node) = self.nodes.iter_mut().find(|node| node.id == id) {
                node.label.push_str(
                    "\n[async future constructed; body not entered without direct await]",
                );
            }
            return Some(id.to_owned());
        }
        let continuation = self.node(
            &format!(
                "after {}{} @ {}:{} (if returned)",
                if candidate { "source candidate " } else { "" },
                index.functions()[j].flow.function,
                site.line,
                site.column
            ),
            site,
        );
        let flow = index.functions()[j].flow.clone();
        let mut forwarded = self.future_profile.zip(source_call).map_or_else(
            std::collections::BTreeMap::new,
            |(profile, call)| {
                super::future_custody::arguments(
                    &index.functions()[f].flow,
                    f,
                    &flow,
                    call,
                    profile,
                    &self.future_values,
                )
            },
        );
        for future in forwarded.values_mut().filter(|future| future.origin == f) {
            future.callbacks.clone_from(&self.callbacks);
        }
        let old_future_values = std::mem::replace(&mut self.future_values, forwarded);
        let previous = self.substitution.clone();
        let old_callbacks = std::mem::take(&mut self.callbacks);
        if callback_target
            && index.callback_bound_unshadowed(j)
            && let Some(call) = source_call
        {
            for parameter in &flow.callback_parameters {
                let bindings = flow
                    .bindings
                    .iter()
                    .filter(|binding| binding.name == parameter.name)
                    .count();
                if bindings == 1
                    && let [closure] = call
                        .async_closures
                        .iter()
                        .filter(|closure| {
                            closure.argument == parameter.argument
                                && closure.parameters.len() == parameter.arity
                        })
                        .collect::<Vec<_>>()
                        .as_slice()
                {
                    self.callbacks.insert(
                        parameter.name.clone(),
                        Rc::new(CallbackBinding {
                            closure: (*closure).clone(),
                            origin: f,
                            captured: old_callbacks.clone(),
                            receiver_types: index
                                .callback_receiver_types(f, j, call, parameter, closure),
                        }),
                    );
                }
            }
        }
        self.substitution = source_call.and_then(|call| {
            if implicit_await {
                index.await_substitution(f, call)
            } else if call.receiver.as_deref() == Some("self") {
                previous
                    .as_ref()
                    .filter(|substitution| index.same_substituted_owner(f, j, substitution))
                    .cloned()
            } else {
                None
            }
        });
        let returned_future = if implicit_await && self.depth >= 2 {
            self.substitution.as_ref().and_then(|substitution| {
                index
                    .returned_pinned_async_target(j, substitution)
                    .filter(|(target, _)| !self.stack.contains(target))
                    .map(|(target, site)| (target, site, substitution.clone()))
            })
        } else {
            None
        };
        let outer_deferred_calls = self.deferred_calls.take();
        let outer_receivers = std::mem::take(&mut self.deferred_receivers);
        let (entry, exit) = self.function(&flow, Some(j), self.depth - 1);
        self.deferred_calls = outer_deferred_calls;
        self.deferred_receivers = outer_receivers;
        self.future_values = old_future_values;
        self.substitution = previous;
        self.callbacks = old_callbacks;
        if async_target {
            let poll = self.node(
                "poll returned async function future [source candidate; completion not guaranteed]",
                site,
            );
            self.edge(id, &poll, Some("if polled"));
            self.edge(&poll, &entry, Some("source candidate"));
        } else {
            self.edge(
                id,
                &entry,
                Some(if candidate {
                    "source candidate"
                } else {
                    "call"
                }),
            );
        }
        if implicit_await {
            let poll = self.node(
                if returned_future.is_some() {
                    "poll returned pinned future [source candidate; completion not guaranteed]"
                } else {
                    "poll returned future [opaque; completion not guaranteed]"
                },
                site,
            );
            self.edge(&exit, &poll, Some("candidate returns future"));
            if let Some((target, _origin, substitution)) = returned_future {
                let previous = self.substitution.replace(substitution);
                let future_flow = index.functions()[target].flow.clone();
                let (future_entry, future_exit) =
                    self.function(&future_flow, Some(target), self.depth - 2);
                self.substitution = previous;
                self.edge(&poll, &future_entry, Some("if polled; source candidate"));
                self.edge(&future_exit, &continuation, Some("if polling completes"));
            } else {
                self.edge(&poll, &continuation, Some("if polling completes"));
            }
        } else {
            self.edge(
                &exit,
                &continuation,
                Some(&format!(
                    "{}return to {}:{}",
                    if candidate { "candidate " } else { "" },
                    site.line,
                    site.column
                )),
            );
        }
        Some(continuation)
    }
    fn steps(&mut self, steps: &[Step], mut tails: Vec<String>) -> Vec<String> {
        for step in steps {
            if self.nodes.len() >= self.budget {
                self.exhausted = true;
                return vec![];
            }
            let label = match step {
                Step::Action {
                    label, fallible, ..
                }
                | Step::Opaque {
                    label, fallible, ..
                } => format!(
                    "{}{}{}",
                    if matches!(step, Step::Opaque { .. }) {
                        "[opaque] "
                    } else {
                        ""
                    },
                    label,
                    if *fallible { " ?" } else { "" }
                ),
                Step::Borrow { label, .. }
                | Step::Branch { label, .. }
                | Step::Match { label, .. }
                | Step::Loop { label, .. }
                | Step::Block { label, .. }
                | Step::AwaitBlock { label, .. }
                | Step::Exit { label, .. } => label.clone(),
            };
            let id = self.node(&label, step.site());
            self.connect(tails, &id);
            if let Some(folds) = &mut self.folds {
                if matches!(step, Step::Borrow { .. }) {
                    folds.linear.push(id.clone());
                }
                if matches!(
                    step,
                    Step::Action {
                        fallible: false,
                        ..
                    }
                ) && folds.call_sites.contains(step.site())
                {
                    folds.calls.insert(id.clone());
                }
            }
            tails = match step {
                Step::Branch { arms, site, .. } => {
                    let mut ends = vec![];
                    for arm in arms {
                        if self.exhausted {
                            break;
                        }
                        let gate = self.node(&arm.label, site);
                        self.edge(&id, &gate, Some(&arm.label));
                        ends.extend(self.steps(&arm.steps, vec![gate]));
                    }
                    ends
                }
                Step::Match { arms, site, .. } => {
                    let mut pending = vec![id];
                    let mut ends = vec![];
                    for arm in arms {
                        if self.exhausted {
                            break;
                        }
                        let pattern = self.node(&format!("pattern: {}", arm.pattern), site);
                        self.connect(pending, &pattern);
                        let next = self.node("try next pattern", site);
                        if arm.pattern != "_" {
                            self.edge(&pattern, &next, Some("does not match"));
                        }
                        let body_gate = self.node("matched arm body", site);
                        if arm.guard.is_empty() {
                            self.edge(&pattern, &body_gate, Some("matches"));
                        } else {
                            let guard = self.node("pattern matches: evaluate guard", site);
                            self.edge(&pattern, &guard, Some("matches"));
                            let guards = self.steps(&arm.guard, vec![guard]);
                            let result = self.node("guard result", site);
                            self.connect(guards, &result);
                            self.edge(&result, &body_gate, Some("true"));
                            self.edge(&result, &next, Some("false"));
                        }
                        ends.extend(self.steps(&arm.body, vec![body_gate]));
                        pending = vec![next];
                    }
                    let unmatched = self.node("match coverage not checked (no fallthrough)", site);
                    self.connect(pending, &unmatched);
                    ends
                }
                Step::Loop {
                    head,
                    body,
                    form,
                    site,
                    target,
                    ..
                } => {
                    // A labelled transfer in a `while` condition names this
                    // loop, so its target is registered before the condition.
                    let exit = self.node("after loop (if it exits)", site);
                    let header = *form == LoopKind::While;
                    if header {
                        self.loops.push(LoopTarget {
                            name: target.clone(),
                            header: id.clone(),
                            exit: exit.clone(),
                            labelled_only: true,
                            block: false,
                        });
                    }
                    let tested = self.steps(head, vec![id.clone()]);
                    if header {
                        self.loops.pop();
                    }
                    let test = self.node("loop header result", site);
                    self.connect(tested, &test);
                    let gate = self.node("loop body (when entered)", site);
                    self.edge(&test, &gate, Some("enter"));
                    if *form != LoopKind::Infinite {
                        self.edge(
                            &test,
                            &exit,
                            Some(if *form == LoopKind::For {
                                "exhausted"
                            } else {
                                "false"
                            }),
                        );
                    }
                    self.loops.push(LoopTarget {
                        name: target.clone(),
                        header: id.clone(),
                        exit: exit.clone(),
                        labelled_only: false,
                        block: false,
                    });
                    let ends = self.steps(body, vec![gate]);
                    self.loops.pop();
                    self.connect(ends, &id);
                    vec![exit]
                }
                Step::AwaitBlock { body, site, .. } => {
                    let end = self.node("async block completes (value or residual)", site);
                    let outer_exit = std::mem::replace(&mut self.exit, end.clone());
                    let outer_loops = std::mem::take(&mut self.loops);
                    let ends = self.steps(body, vec![id.clone()]);
                    self.connect(ends, &end);
                    self.loops = outer_loops;
                    self.exit = outer_exit;
                    vec![end]
                }
                Step::Block {
                    body, target, site, ..
                } => {
                    let exit = self.node("after labelled block", site);
                    self.loops.push(LoopTarget {
                        name: Some(target.clone()),
                        header: id.clone(),
                        exit: exit.clone(),
                        labelled_only: true,
                        block: true,
                    });
                    let ends = self.steps(body, vec![id.clone()]);
                    self.loops.pop();
                    self.connect(ends, &exit);
                    vec![exit]
                }
                Step::Exit { control, .. } => {
                    let target = match control {
                        ExitKind::Return => Some(self.exit.clone()),
                        ExitKind::Break(name) | ExitKind::Continue(name) => self
                            .loops
                            .iter()
                            .rev()
                            // An unlabelled transfer never names a block.
                            .find(|t| match name {
                                Some(n) => t.name.as_ref() == Some(n),
                                None => !t.block,
                            })
                            .filter(|t| name.is_some() || !t.labelled_only)
                            // `continue 'label` cannot name a block.
                            .filter(|t| !(t.block && matches!(control, ExitKind::Continue(_))))
                            .map(|t| {
                                if matches!(control, ExitKind::Break(_)) {
                                    t.exit.clone()
                                } else {
                                    t.header.clone()
                                }
                            }),
                    };
                    if let Some(target) = target {
                        self.edge(&id, &target, Some("transfer"));
                    } else {
                        let unknown =
                            self.node("opaque control target (not resolved)", step.site());
                        self.connect(vec![id], &unknown);
                    }
                    vec![]
                }
                Step::Action {
                    fallible: true,
                    site,
                    ..
                }
                | Step::Opaque {
                    fallible: true,
                    site,
                    ..
                } => {
                    if let Some(folds) = &mut self.folds {
                        let binding = if let Step::Action { binding, .. } = step {
                            binding.clone()
                        } else {
                            None
                        };
                        folds.residuals.push((id.clone(), binding));
                    }
                    let early = self.node("? returns early (residual)", site);
                    if let Some(folds) = &mut self.folds {
                        folds.linear.push(early.clone());
                    }
                    self.edge(&id, &early, Some("residual"));
                    let exit = self.exit.clone();
                    self.edge(&early, &exit, Some("return residual"));
                    let success = self.node("? continues", site);
                    if let Some(folds) = &mut self.folds {
                        folds.linear.push(success.clone());
                    }
                    self.edge(&id, &success, Some("success"));
                    vec![success]
                }
                Step::Action { site, .. } => vec![self.call(&id, site).unwrap_or(id)],
                Step::Borrow { .. } | Step::Opaque { .. } => vec![id],
            };
        }
        tails
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn return_arm_does_not_flow_into_following_statement() {
        let flow =
            crate::workflow::parse::parse("fn f(){if x {return early();} late();}", "lib.rs", "f")
                .unwrap();
        let diagram = flow.diagram();
        diagram.validate().unwrap();
        let ret = diagram
            .nodes
            .iter()
            .find(|n| n.label.starts_with("return"))
            .unwrap();
        assert!(diagram.edges.iter().filter(|e| e.from == ret.id).all(|e| {
            diagram
                .nodes
                .iter()
                .find(|n| n.id == e.to)
                .unwrap()
                .label
                .starts_with("return from")
        }));
        assert!(
            diagram
                .edges
                .iter()
                .all(|e| matches!(e.evidence, EdgeEvidence::SourceOrder))
        );
    }
    #[test]
    fn fallible_step_has_explicit_early_return_sink() {
        let flow =
            crate::workflow::parse::parse("fn f(){let x=read()?; consume(x);}", "lib.rs", "f")
                .unwrap();
        let diagram = flow.diagram();
        let early = diagram
            .nodes
            .iter()
            .find(|n| n.label.starts_with("? returns"))
            .unwrap();
        assert!(diagram.edges.iter().any(|e| e.to == early.id));
        assert!(
            diagram
                .edges
                .iter()
                .filter(|e| e.from == early.id)
                .all(|e| diagram
                    .nodes
                    .iter()
                    .find(|n| n.id == e.to)
                    .unwrap()
                    .label
                    .starts_with("return from"))
        );
    }

    #[test]
    fn directly_awaited_block_has_its_own_residual_and_return_target() {
        let flow = crate::workflow::parse::parse(
            "async fn f(){let result = async { if stop { return fail(); } read().await?; Ok(()) }.await; after(result)?;}",
            "lib.rs",
            "f",
        ).unwrap();
        let diagram = flow.diagram();
        diagram.validate().unwrap();
        let block_end = diagram
            .nodes
            .iter()
            .find(|n| n.label == "async block completes (value or residual)")
            .unwrap();
        let function_end = diagram
            .nodes
            .iter()
            .find(|n| n.label.starts_with("return from f"))
            .unwrap();
        let ret = diagram
            .nodes
            .iter()
            .find(|n| n.label == "return fail()")
            .unwrap();
        assert!(
            diagram
                .edges
                .iter()
                .any(|e| e.from == ret.id && e.to == block_end.id)
        );
        let early: Vec<_> = diagram
            .nodes
            .iter()
            .filter(|n| n.label == "? returns early (residual)")
            .collect();
        assert_eq!(early.len(), 2);
        assert!(
            diagram
                .edges
                .iter()
                .any(|e| e.from == early[0].id && e.to == block_end.id)
        );
        assert!(
            diagram
                .edges
                .iter()
                .any(|e| e.from == early[1].id && e.to == function_end.id)
        );
        let after = diagram
            .nodes
            .iter()
            .find(|n| n.label.contains("await completed"))
            .unwrap();
        assert!(
            diagram
                .edges
                .iter()
                .any(|e| e.from == block_end.id && e.to == after.id)
        );
    }

    #[test]
    fn question_after_await_propagates_only_after_the_future_completes() {
        let flow = crate::workflow::parse::parse(
            "async fn f(){ let x = async { read().await?; Ok(()) }.await?; done(x); }",
            "lib.rs",
            "f",
        )
        .unwrap();
        let diagram = flow.diagram();
        diagram.validate().unwrap();
        let block_end = diagram
            .nodes
            .iter()
            .find(|n| n.label == "async block completes (value or residual)")
            .unwrap();
        let function_end = diagram
            .nodes
            .iter()
            .find(|n| n.label.starts_with("return from f"))
            .unwrap();
        let early: Vec<_> = diagram
            .nodes
            .iter()
            .filter(|n| n.label == "? returns early (residual)")
            .collect();
        assert_eq!(early.len(), 2);
        assert!(
            diagram
                .edges
                .iter()
                .any(|e| e.from == early[0].id && e.to == block_end.id)
        );
        assert!(
            diagram
                .edges
                .iter()
                .any(|e| e.from == early[1].id && e.to == function_end.id)
        );
        let await_done = diagram
            .nodes
            .iter()
            .find(|n| n.label == "await completed")
            .unwrap();
        assert!(
            diagram
                .edges
                .iter()
                .any(|e| e.from == block_end.id && e.to == await_done.id)
        );
        let outer_try = diagram
            .edges
            .iter()
            .find(|e| e.from == await_done.id)
            .unwrap();
        assert!(
            diagram
                .edges
                .iter()
                .any(|e| e.from == outer_try.to && e.to == early[1].id)
        );
    }
}

#[cfg(test)]
mod loop_tests {
    #[test]
    fn break_exits_its_loop_and_continue_repeats_it() {
        let flow = crate::workflow::parse::parse(
            "fn f(){'outer: loop { loop {if stop {break 'outer;} continue;} } after();}",
            "lib.rs",
            "f",
        )
        .unwrap();
        let diagram = flow.diagram();
        diagram.validate().unwrap();
        let jump = diagram
            .nodes
            .iter()
            .find(|n| n.label == "break 'outer")
            .unwrap();
        let edge = diagram.edges.iter().find(|e| e.from == jump.id).unwrap();
        let outer_exit = diagram.nodes.iter().find(|n| n.id == edge.to).unwrap();
        assert!(outer_exit.label.starts_with("after loop"));
        let after = diagram.nodes.iter().find(|n| n.label == "after()").unwrap();
        assert!(
            diagram
                .edges
                .iter()
                .any(|e| e.from == outer_exit.id && e.to == after.id)
        );
        let jump = diagram
            .nodes
            .iter()
            .find(|n| n.label == "continue")
            .unwrap();
        let edge = diagram.edges.iter().find(|e| e.from == jump.id).unwrap();
        let header = diagram.nodes.iter().find(|n| n.id == edge.to).unwrap();
        assert_eq!(header.label, "loop");
        assert_ne!(header.id, "flow-1");
    }

    /// The target of each transfer whose label starts with `jump`.
    fn targets(source: &str, jump: &str) -> Vec<String> {
        let diagram = crate::workflow::parse::parse(source, "lib.rs", "f")
            .unwrap()
            .diagram();
        diagram.validate().unwrap();
        diagram
            .nodes
            .iter()
            .filter(|n| n.label.starts_with(jump))
            .map(|n| {
                let edge = diagram.edges.iter().find(|e| e.from == n.id).unwrap();
                let to = diagram.nodes.iter().find(|t| t.id == edge.to).unwrap();
                to.label.clone()
            })
            .collect()
    }

    #[test]
    fn labelled_transfers_in_a_while_condition_reach_their_loop() {
        let source = "fn f(){'a: while { if p() { continue 'a; } q() } { 'b: while { if p() { break 'a; } if q() { continue 'b; } r() } { s(); } } after();}";
        // A loop's header node is labelled with its condition.
        let continues = targets(source, "continue");
        assert_eq!(continues.len(), 2, "{continues:?}");
        assert!(continues[0].starts_with("while { if p() { continue 'a"));
        assert!(continues[1].starts_with("while { if p() { break 'a"));
        assert_eq!(targets(source, "break 'a"), ["after loop (if it exits)"]);
    }

    #[test]
    fn break_label_leaves_a_labelled_block_and_plain_transfers_skip_it() {
        let source = "fn f(){ loop { 'found: { if p() { break 'found; } if q() { break; } if r() { continue; } work(); } after(); } }";
        assert_eq!(targets(source, "break 'found"), ["after labelled block"]);
        // The unlabelled break and continue name the loop, not the block.
        assert_eq!(
            targets(source, "break"),
            ["after labelled block", "after loop (if it exits)"]
        );
        assert_eq!(targets(source, "continue"), ["loop"]);
        let diagram = crate::workflow::parse::parse(source, "lib.rs", "f")
            .unwrap()
            .diagram();
        let after_block = diagram
            .nodes
            .iter()
            .find(|n| n.label == "after labelled block")
            .unwrap();
        let after = diagram.nodes.iter().find(|n| n.label == "after()").unwrap();
        assert!(
            diagram
                .edges
                .iter()
                .any(|e| e.from == after_block.id && e.to == after.id)
        );
    }

    #[test]
    fn continue_cannot_name_a_labelled_block() {
        let source = "fn f(){ 'b: { if p() { continue 'b; } } }";
        assert_eq!(
            targets(source, "continue"),
            ["opaque control target (not resolved)"]
        );
    }

    #[test]
    fn an_unlabelled_transfer_in_a_while_condition_is_not_resolved() {
        // Rust refuses it; Atlas must not pick the enclosing loop instead.
        let source = "fn f(){ loop { while { if p() { continue; } q() } { s(); } } }";
        assert_eq!(
            targets(source, "continue"),
            ["opaque control target (not resolved)"]
        );
    }
}
