//! Executable source witnesses qualify the structural observations, not feasibility inference.
mod support;
use cargo_atlas::{diagram::Diagram, discovery, workflow};
use std::collections::BTreeSet;
use support::Fixture;

fn inspect(files: &[(&str, &str)], function: &str, depth: u8) -> (Fixture, Diagram) {
    let f = Fixture::new(files);
    let m = cargo_metadata::MetadataCommand::new()
        .current_dir(f.path())
        .no_deps()
        .exec()
        .expect("test fixture prerequisite must succeed");
    let graph = discovery::scan_metadata(&m).expect("test fixture prerequisite must succeed");
    let (_, d) = workflow::expanded(f.path(), &m, &graph, "src/lib.rs", function, depth)
        .expect("test fixture prerequisite must succeed");
    d.validate()
        .expect("test fixture prerequisite must succeed");
    (f, d)
}
fn reachable(d: &Diagram, from: &str, to: &str) -> bool {
    let mut todo = vec![from.to_owned()];
    let mut seen = BTreeSet::new();
    while let Some(n) = todo.pop() {
        if n == to {
            return true;
        }
        if seen.insert(n.clone()) {
            todo.extend(d.edges.iter().filter(|e| e.from == n).map(|e| e.to.clone()));
        }
    }
    false
}

#[test]
fn written_field_and_literal_receivers_expand_unique_inherent_methods() {
    let (_, d) = inspect(
        &[
            (
                "src/lib.rs",
                "mod model; mod service; impl model::Owner { pub fn run(&self) { self.auth.resolve(); } }",
            ),
            (
                "src/model.rs",
                "use crate::service::Auth; pub struct Owner { pub auth: Auth }",
            ),
            (
                "src/service.rs",
                "pub struct Auth; impl Auth { pub fn resolve(&self) {} }",
            ),
        ],
        "model::Owner::run",
        1,
    );
    assert!(d.nodes.iter().any(|n| n.label == "fn Auth::resolve"));
    let (_, d) = inspect(
        &[
            (
                "src/lib.rs",
                "mod model; pub use model::*; pub fn run(){ Request {}.execute(); }",
            ),
            (
                "src/model.rs",
                "pub struct Request {} impl Request { pub fn execute(&self) {} }",
            ),
        ],
        "run",
        1,
    );
    assert!(d.nodes.iter().any(|n| n.label == "fn Request::execute"));
}

#[test]
fn receiver_lookup_refuses_ambiguous_globs_and_generic_field_types() {
    let (_, d) = inspect(
        &[
            (
                "src/lib.rs",
                "mod a; mod b; pub use a::*; pub use b::*; pub fn run(){ Request {}.execute(); }",
            ),
            (
                "src/a.rs",
                "pub struct Request {} impl Request { pub fn execute(&self) {} }",
            ),
            (
                "src/b.rs",
                "pub struct Request {} impl Request { pub fn execute(&self) {} }",
            ),
        ],
        "run",
        1,
    );
    assert!(!d.nodes.iter().any(|n| n.label == "fn Request::execute"));
    assert!(
        d.nodes
            .iter()
            .any(|n| n.label.contains("struct declaration not unique"))
    );
    let (_, d) = inspect(
        &[(
            "src/lib.rs",
            "struct Owner<T>{ value:T } impl<T> Owner<T> { fn run(&self){ self.value.execute(); } }",
        )],
        "Owner<T>::run",
        1,
    );
    assert!(!d.nodes.iter().any(|n| n.label == "fn T::execute"));
}

#[test]
fn public_glob_free_function_is_a_candidate_only_when_unique() {
    let (_, diagram) = inspect(
        &[
            (
                "src/lib.rs",
                "mod inner; pub use inner::*; pub fn run() { execute(); }",
            ),
            ("src/inner.rs", "pub fn execute() {}"),
        ],
        "run",
        1,
    );
    assert!(diagram.nodes.iter().any(|n| n.label == "fn execute"));
    assert!(
        diagram
            .edges
            .iter()
            .any(|e| e.label.as_deref() == Some("source candidate"))
    );
    let (_, private) = inspect(
        &[
            (
                "src/lib.rs",
                "mod inner; pub use inner::*; pub fn run() { execute(); }",
            ),
            ("src/inner.rs", "fn execute() {}"),
        ],
        "run",
        1,
    );
    assert!(!private.nodes.iter().any(|n| n.label == "fn execute"));
    let (_, ambiguous) = inspect(
        &[
            (
                "src/lib.rs",
                "mod a; mod b; pub use a::*; pub use b::*; pub fn run() { execute(); }",
            ),
            ("src/a.rs", "pub fn execute() {}"),
            ("src/b.rs", "pub fn execute() {}"),
        ],
        "run",
        1,
    );
    assert!(!ambiguous.nodes.iter().any(|n| n.label == "fn execute"));
    let (_, reexport_ambiguity) = inspect(
        &[
            (
                "src/lib.rs",
                "mod a; mod b; mod c; pub use b::*; pub use c::*; pub fn run() { execute(); }",
            ),
            ("src/a.rs", "pub fn execute() {}"),
            ("src/b.rs", "pub use crate::a::execute;"),
            ("src/c.rs", "pub fn execute() {}"),
        ],
        "run",
        1,
    );
    assert!(
        !reexport_ambiguity
            .nodes
            .iter()
            .any(|n| n.label == "fn execute")
    );
}

#[test]
fn glob_candidate_crosses_packages_and_is_recomputed_after_source_changes() {
    let fixture = Fixture::new(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers=['app','domain']\nresolver='3'\n",
        ),
        (
            "app/Cargo.toml",
            "[package]\nname='app'\nversion='0.1.0'\nedition='2024'\n[dependencies]\ndomain={path='../domain'}\n",
        ),
        (
            "domain/Cargo.toml",
            "[package]\nname='domain'\nversion='0.1.0'\nedition='2024'\n",
        ),
        (
            "app/src/lib.rs",
            "use domain::dispatch; pub fn run() { dispatch(); }",
        ),
        ("domain/src/lib.rs", "mod inner; pub use inner::*;"),
        ("domain/src/inner.rs", "pub fn dispatch() {}"),
    ]);
    let generate = || {
        let metadata = cargo_metadata::MetadataCommand::new()
            .current_dir(fixture.path())
            .no_deps()
            .exec()
            .unwrap();
        let graph = discovery::scan_metadata(&metadata).unwrap();
        workflow::expanded(
            fixture.path(),
            &metadata,
            &graph,
            "app/src/lib.rs",
            "run",
            1,
        )
        .unwrap()
        .1
    };
    let before = generate();
    assert!(before.nodes.iter().any(|n| n.label == "fn dispatch"));
    assert!(
        before
            .edges
            .iter()
            .any(|e| e.label.as_deref() == Some("source candidate"))
    );
    fixture.write("domain/src/inner.rs", "pub fn renamed() {}");
    let after = generate();
    assert!(!after.nodes.iter().any(|n| n.label == "fn dispatch"));
    assert!(
        !after
            .edges
            .iter()
            .any(|e| e.label.as_deref() == Some("source candidate"))
    );
}

#[test]
fn direct_self_calls_in_awaited_blocks_are_bounded_source_candidates() {
    let (_, diagram) = inspect(
        &[(
            "src/lib.rs",
            "struct H; impl H { pub async fn run(&self) { async { self.page::<u8>().await; }.await; } async fn page<T>(&self) {} }",
        )],
        "H::run",
        1,
    );
    assert!(diagram.nodes.iter().any(|n| n.label == "fn H::page"));
    assert!(diagram.edges.iter().any(|e| {
        e.label.as_deref() == Some("source candidate")
            && diagram
                .nodes
                .iter()
                .any(|n| n.id == e.from && n.label.contains("self.page::<u8>().await"))
    }));
    let (_, deferred) = inspect(
        &[(
            "src/lib.rs",
            "struct H; impl H { pub async fn run(&self) { let later = async { self.page::<u8>().await }; let callback = || self.page::<u8>(); } async fn page<T>(&self) {} }",
        )],
        "H::run",
        1,
    );
    assert!(!deferred.nodes.iter().any(|n| n.label == "fn H::page"));
    let (_, dropped_future) = inspect(
        &[(
            "src/lib.rs",
            "struct H; impl H { pub async fn run(&self) { async { self.page::<u8>(); }.await; } async fn page<T>(&self) {} }",
        )],
        "H::run",
        1,
    );
    assert!(!dropped_future.nodes.iter().any(|n| n.label == "fn H::page"));
    let (_, sync_constructor) = inspect(
        &[(
            "src/lib.rs",
            "struct H; impl H { pub async fn run(&self) { async { self.page().await; }.await; } fn page(&self) -> impl Future<Output=()> { async {} } }",
        )],
        "H::run",
        1,
    );
    assert!(
        !sync_constructor
            .nodes
            .iter()
            .any(|n| n.label == "fn H::page")
    );
    let (_, trait_only) = inspect(
        &[(
            "src/lib.rs",
            "trait T { async fn page(&self) {} } struct H; impl T for H {} impl H { pub async fn run(&self) { async { self.page().await; }.await; } }",
        )],
        "H::run",
        1,
    );
    assert!(!trait_only.nodes.iter().any(|n| n.label == "fn T::page"));
    assert!(
        !trait_only
            .edges
            .iter()
            .any(|e| e.label.as_deref() == Some("source candidate"))
    );
}

#[test]
fn awaited_paths_in_async_blocks_refuse_shadowed_and_sync_targets() {
    let (_, diagram) = inspect(
        &[(
            "src/lib.rs",
            "async fn page() {} pub async fn run() { async { page().await; }.await; }",
        )],
        "run",
        1,
    );
    assert!(diagram.nodes.iter().any(|n| n.label == "fn page"));
    assert!(diagram.edges.iter().any(|e| {
        e.label.as_deref() == Some("source candidate")
            && diagram
                .nodes
                .iter()
                .any(|n| n.id == e.from && n.label.contains("page().await"))
    }));
    let (_, shadowed) = inspect(
        &[(
            "src/lib.rs",
            "async fn page() {} pub async fn run() { async { let page = || async {}; page().await; }.await; }",
        )],
        "run",
        1,
    );
    assert!(!shadowed.nodes.iter().any(|n| n.label == "fn page"));
    let (_, sync_constructor) = inspect(
        &[(
            "src/lib.rs",
            "fn page() -> impl Future<Output=()> { async {} } pub async fn run() { async { page().await; }.await; }",
        )],
        "run",
        1,
    );
    assert!(!sync_constructor.nodes.iter().any(|n| n.label == "fn page"));
    let (_, dropped) = inspect(
        &[(
            "src/lib.rs",
            "async fn page() {} pub async fn run() { async { page(); }.await; }",
        )],
        "run",
        1,
    );
    assert!(!dropped.nodes.iter().any(|n| n.label == "fn page"));
}

#[test]
fn trait_defaults_and_overrides_are_explicit_source_candidates() {
    let (_, d) = inspect(
        &[
            (
                "src/lib.rs",
                "mod api; mod types; use api::Execute; use types::Request; pub fn run(){ Request {}.execute(); }",
            ),
            (
                "src/api.rs",
                "pub trait Execute { fn execute(&self) { default_body(); } } fn default_body() {}",
            ),
            (
                "src/types.rs",
                "use crate::api::Execute; pub struct Request {} impl Execute for Request {}",
            ),
        ],
        "run",
        1,
    );
    assert!(d.nodes.iter().any(|n| n.label == "fn Execute::execute"));
    assert!(
        d.edges
            .iter()
            .any(|e| e.label.as_deref() == Some("source candidate"))
    );
    assert!(d.edges.iter().any(|e| {
        e.label
            .as_deref()
            .is_some_and(|label| label.starts_with("candidate return to"))
    }));
    assert!(
        d.nodes
            .iter()
            .any(|n| n.label.contains("not proved dispatch"))
    );

    let (_, d) = inspect(
        &[(
            "src/lib.rs",
            "trait Execute { fn execute(&self) { default_body(); } } struct Request {} impl Execute for Request { fn execute(&self) { override_body(); } } fn default_body() {} fn override_body() {} pub fn run(){ Request {}.execute(); }",
        )],
        "run",
        1,
    );
    assert!(d.nodes.iter().any(|n| n.label == "fn Request::execute"));
    assert!(!d.nodes.iter().any(|n| n.label == "fn Execute::execute"));
}

#[test]
fn trait_candidates_refuse_ambiguity_and_out_of_scope_traits() {
    let (_, d) = inspect(
        &[(
            "src/lib.rs",
            "trait One { fn execute(&self) {} } trait Two { fn execute(&self) {} } struct Request {} impl One for Request {} impl Two for Request {} pub fn run(){ Request {}.execute(); }",
        )],
        "run",
        1,
    );
    assert!(
        !d.edges
            .iter()
            .any(|e| e.label.as_deref() == Some("source candidate"))
    );
    assert!(
        d.nodes
            .iter()
            .any(|n| n.label.contains("ambiguous in-scope trait candidates"))
    );
    let (_, d) = inspect(
        &[
            (
                "src/lib.rs",
                "mod api; mod types; use types::Request; pub fn run(){ Request {}.execute(); }",
            ),
            ("src/api.rs", "pub trait Execute { fn execute(&self) {} }"),
            (
                "src/types.rs",
                "use crate::api::Execute; pub struct Request {} impl Execute for Request {}",
            ),
        ],
        "run",
        1,
    );
    assert!(!d.nodes.iter().any(|n| n.label == "fn Execute::execute"));
}

#[test]
fn trait_candidates_do_not_override_inherent_or_generic_uncertainty() {
    let (_, d) = inspect(
        &[(
            "src/lib.rs",
            "trait Execute { fn execute(&self); } struct Request {} impl Execute for Request { fn execute(&self) {} } impl Request { fn execute(&self) {} } pub fn run(){ Request {}.execute(); }",
        )],
        "run",
        1,
    );
    assert!(
        !d.edges
            .iter()
            .any(|e| e.label.as_deref() == Some("source candidate"))
    );
    assert!(
        d.nodes
            .iter()
            .any(|n| n.label.contains("competing inherent method observations"))
    );
    let (_, d) = inspect(
        &[(
            "src/lib.rs",
            "trait Execute { fn execute(&self) {} } struct Request {} impl<T> Execute for T {} pub fn run(){ Request {}.execute(); }",
        )],
        "run",
        1,
    );
    assert!(
        !d.edges
            .iter()
            .any(|e| e.label.as_deref() == Some("source candidate"))
    );
    assert!(
        d.nodes
            .iter()
            .any(|n| n.label.contains("generic trait implementation may apply"))
    );
}

#[test]
fn written_receivers_cross_package_exports_with_explicit_evidence() {
    let fixture = Fixture::new(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers=['app','domain']\nresolver='3'\n",
        ),
        (
            "app/Cargo.toml",
            "[package]\nname='app'\nversion='0.1.0'\nedition='2024'\n[dependencies]\ndomain={path='../domain'}\n",
        ),
        (
            "domain/Cargo.toml",
            "[package]\nname='domain'\nversion='0.1.0'\nedition='2024'\n",
        ),
        (
            "app/src/lib.rs",
            "use domain::{Auth, Execute, Request}; pub struct Owner { auth: Auth } impl Owner { pub fn run(&self) { self.auth.resolve(); Request {}.execute(); } }",
        ),
        (
            "domain/src/lib.rs",
            "mod api; mod types; pub use api::Execute; pub use types::*;",
        ),
        (
            "domain/src/api.rs",
            "pub trait Execute { fn execute(&self) {} }",
        ),
        (
            "domain/src/types.rs",
            "use crate::api::Execute; pub struct Auth; impl Auth { pub fn resolve(&self) {} } pub struct Request {} impl Execute for Request {}",
        ),
    ]);
    let metadata = cargo_metadata::MetadataCommand::new()
        .current_dir(fixture.path())
        .no_deps()
        .exec()
        .unwrap();
    let graph = discovery::scan_metadata(&metadata).unwrap();
    let (_, d) = workflow::expanded(
        fixture.path(),
        &metadata,
        &graph,
        "app/src/lib.rs",
        "Owner::run",
        1,
    )
    .unwrap();
    d.validate().unwrap();
    assert!(d.nodes.iter().any(|n| n.label == "fn Auth::resolve"));
    assert!(d.nodes.iter().any(|n| n.label == "fn Execute::execute"));
    assert!(
        d.edges
            .iter()
            .any(|e| e.label.as_deref() == Some("source candidate"))
    );
}
const CONTROL: &str = r#"
use std::sync::atomic::{AtomicUsize,Ordering};
static GUARDS:AtomicUsize=AtomicUsize::new(0);
fn g()->Result<usize,()>{Err(())}
fn h()->usize{panic!("h must not run after g's residual")}
fn combine(_:usize,_:usize){panic!("combine must not run after g's residual")}
fn nested()->Result<(),()>{combine(g()?,h());Ok(())}
fn predicate(_:usize)->bool{GUARDS.fetch_add(1,Ordering::SeqCst);true}
fn guarded(x:Option<usize>)->usize{match x{Some(v) if predicate(v)=>v,_=>0}}
fn main(){assert_eq!(nested(),Err(()));assert_eq!(guarded(None),0);assert_eq!(GUARDS.load(Ordering::SeqCst),0);}
"#;
#[test]
fn nested_residual_and_pattern_failure_skip_their_following_work() {
    let (f, d) = inspect(&[("src/lib.rs", CONTROL)], "nested", 1);
    let binary = f.path().join("control");
    let compile = std::process::Command::new("rustc")
        .args(["--edition=2024"])
        .arg(f.path().join("src/lib.rs"))
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    assert!(
        std::process::Command::new(binary)
            .status()
            .unwrap()
            .success()
    );
    let residual = d
        .nodes
        .iter()
        .find(|n| n.label == "? returns early (residual)")
        .unwrap();
    for skipped in d
        .nodes
        .iter()
        .filter(|n| n.label.starts_with("h()") || n.label == "combine(g()?,h())")
    {
        assert!(!reachable(&d, &residual.id, &skipped.id));
    }
    let root = d.nodes.iter().find(|n| n.label == "fn nested").unwrap();
    let h = d.nodes.iter().find(|n| n.label.starts_with("h()")).unwrap();
    assert!(
        reachable(&d, &root.id, &h.id),
        "success branch remains a source possibility; no constant-result inference"
    );
    let (_, d) = inspect(&[("src/lib.rs", CONTROL)], "guarded", 1);
    let pattern = d
        .nodes
        .iter()
        .find(|n| n.label == "pattern: Some(v)")
        .unwrap();
    let failed = d
        .edges
        .iter()
        .find(|e| e.from == pattern.id && e.label.as_deref() == Some("does not match"))
        .unwrap();
    assert!(
        d.nodes.iter().any(|n| n.label == "fn predicate"),
        "guard call resolves through the shared index"
    );
    let guard = d
        .nodes
        .iter()
        .find(|n| n.label.starts_with("predicate(v)"))
        .unwrap();
    assert!(!reachable(&d, &failed.to, &guard.id));
    let result = d.nodes.iter().find(|n| n.label == "guard result").unwrap();
    assert!(
        d.edges.iter().any(|e| e.from == result.id
            && e.to == failed.to
            && e.label.as_deref() == Some("false"))
    );
}
#[test]
fn repeated_cross_file_calls_have_distinct_matched_returns() {
    let (_, d) = inspect(
        &[
            (
                "src/lib.rs",
                "mod child; pub fn run(){child::work();child::work();}",
            ),
            ("src/child.rs", "pub fn work(){}"),
        ],
        "run",
        1,
    );
    let returns: Vec<_> = d
        .edges
        .iter()
        .filter(|e| {
            e.label
                .as_deref()
                .is_some_and(|s| s.starts_with("return to"))
        })
        .collect();
    assert_eq!(returns.len(), 2);
    assert_ne!(returns[0].from, returns[1].from);
    assert_ne!(returns[0].to, returns[1].to);
    for edge in returns {
        let exit = d.nodes.iter().find(|n| n.id == edge.from).unwrap();
        assert_eq!(exit.group, "src/child.rs");
        let continuation = d.nodes.iter().find(|n| n.id == edge.to).unwrap();
        assert_eq!(continuation.group, "src/lib.rs");
        assert_eq!(d.edges.iter().filter(|e| e.from == edge.from).count(), 1);
    }
}
#[test]
fn loop_forms_preserve_exhaustion_condition_rechecks_and_absence_of_infinite_exit() {
    let (_, d) = inspect(
        &[(
            "src/lib.rs",
            "pub fn run(){loop {continue;} after();} fn after(){}",
        )],
        "run",
        1,
    );
    let entry = d.nodes.iter().find(|n| n.label == "fn run").unwrap();
    let after = d.nodes.iter().find(|n| n.label == "after()").unwrap();
    assert!(!reachable(&d, &entry.id, &after.id));
    let (_, d) = inspect(
        &[(
            "src/lib.rs",
            "pub fn run(){while ready(){continue;} for x in values(){consume(x);} } fn ready()->bool{true} fn values()->[u8;0]{[]} fn consume(_:u8){}",
        )],
        "run",
        1,
    );
    let head = d.nodes.iter().find(|n| n.label == "while ready()").unwrap();
    let condition = d
        .nodes
        .iter()
        .find(|n| n.label.starts_with("ready()"))
        .unwrap();
    let jump = d.nodes.iter().find(|n| n.label == "continue").unwrap();
    assert!(d.edges.iter().any(|e| e.from == jump.id && e.to == head.id));
    assert!(reachable(&d, &head.id, &condition.id));
    assert_eq!(
        d.nodes
            .iter()
            .filter(|n| n.label.starts_with("values()"))
            .count(),
        1
    );
    assert!(
        d.edges
            .iter()
            .any(|e| e.label.as_deref() == Some("exhausted"))
    );
}
#[test]
fn recursion_depth_unknowns_and_return_operands_are_explicit() {
    let (_, d) = inspect(
        &[(
            "src/lib.rs",
            "pub fn recur(x:u8)->u8{if x==0{return external::value();}recur(x-1)}",
        )],
        "recur",
        3,
    );
    assert!(
        d.nodes
            .iter()
            .any(|n| n.label.contains("recursion boundary"))
    );
    assert!(
        d.nodes
            .iter()
            .any(|n| n.label.contains("not expanded:") && !n.label.contains("recursion"))
    );
    let call = d
        .nodes
        .iter()
        .find(|n| n.label.starts_with("external::value()"))
        .unwrap();
    let transfer = d
        .nodes
        .iter()
        .find(|n| n.label == "return external::value()")
        .unwrap();
    assert!(reachable(&d, &call.id, &transfer.id));
}

#[test]
fn large_empty_match_arms_refuse_the_node_budget_instead_of_evading_it() {
    use std::fmt::Write as _;
    let mut arms = String::new();
    for index in 0..210 {
        write!(arms, "{index}=>{{}},").expect("formatting into a String cannot fail");
    }
    let source = format!("pub fn run(x:u16){{match x{{{arms}_=>{{}}}}}}");
    let f = Fixture::new(&[("src/lib.rs", &source)]);
    let m = cargo_metadata::MetadataCommand::new()
        .current_dir(f.path())
        .no_deps()
        .exec()
        .unwrap();
    let graph = discovery::scan_metadata(&m).unwrap();
    let error = workflow::expanded(f.path(), &m, &graph, "src/lib.rs", "run", 1).unwrap_err();
    assert!(error.to_string().contains("600"));
}

#[test]
fn compact_control_preserves_retained_paths_and_every_folded_observation() {
    for source in [
        CONTROL,
        "pub fn run(x:Option<u8>){match x{Some(v) if guard(&v)=>take(v),_=>()}} fn guard(_: &u8)->bool{true} fn take(_:u8){}",
        "mod child {pub fn work()->Result<(),()>{Ok(())}} pub fn run()->Result<(),()>{child::work()?;child::work()?;Ok(())}",
        "pub fn run(){for x in values(){if ready(){continue;} consume(&x);} opaque!();} fn ready()->bool{true} fn values()->[u8;0]{[]} fn consume(_: &u8){}",
        "pub fn run()->Result<(),()>{combine(first()?, &second());Ok(())} fn first()->Result<u8,()>{Err(())} fn second()->u8{0} fn combine(_:u8,_:&u8){}",
        "pub fn run(){loop {continue;} after();} fn after(){}",
        "pub fn run(){run();}",
    ] {
        let function = if source == CONTROL { "nested" } else { "run" };
        let fixture = Fixture::new(&[("src/lib.rs", source)]);
        let metadata = cargo_metadata::MetadataCommand::new()
            .current_dir(fixture.path())
            .no_deps()
            .exec()
            .unwrap();
        let graph = discovery::scan_metadata(&metadata).unwrap();
        let architecture = cargo_atlas::architecture::Architecture {
            schema: 1,
            provenance: vec![],
            modules: vec![],
            workflows: vec![],
        };
        let index =
            atlas_lib::source_index::Index::new(fixture.path(), &metadata, &graph, &architecture)
                .unwrap();
        for depth in [0, 1] {
            let (_, raw) = workflow::expand(&index, "src/lib.rs", function, depth).unwrap();
            let (_, compact) = workflow::expand_with_detail(
                &index,
                "src/lib.rs",
                function,
                depth,
                workflow::Detail::Control,
            )
            .unwrap();
            compact.validate().unwrap();
            let accounted: BTreeSet<_> = compact
                .nodes
                .iter()
                .chain(&compact.coverage.summarized_nodes)
                .map(|n| n.id.as_str())
                .collect();
            assert_eq!(accounted, raw.nodes.iter().map(|n| n.id.as_str()).collect());
            assert_eq!(
                compact.nodes.len() + compact.coverage.summarized_nodes.len(),
                raw.nodes.len()
            );
            for node in &compact.coverage.summarized_nodes {
                let original = raw.nodes.iter().find(|n| n.id == node.id).unwrap();
                assert_eq!(node.label, original.label);
                assert_eq!(node.source_sites, original.source_sites);
            }
            for from in &compact.nodes {
                for to in &compact.nodes {
                    assert_eq!(
                        reachable(&raw, &from.id, &to.id),
                        reachable(&compact, &from.id, &to.id),
                        "{} → {} at depth {depth}",
                        from.label,
                        to.label
                    );
                }
            }
            for edge in raw.edges.iter().filter(|e| {
                e.label
                    .as_deref()
                    .is_some_and(|s| s.starts_with("return to"))
            }) {
                assert!(
                    compact
                        .edges
                        .iter()
                        .any(|e| e.from == edge.from && e.to == edge.to && e.label == edge.label)
                );
            }
        }
    }
}

#[test]
fn control_cli_folds_mechanics_and_retains_summary_custody() {
    let fixture = Fixture::new(&[(
        "src/lib.rs",
        "pub fn run()->Result<(),()>{let x = first(&1)?; second(&x); Ok(())} fn first(_: &u8)->Result<u8,()>{Ok(1)} fn second(_: &u8){} fn other()->Result<(),()>{ first(&2)?; Ok(()) }",
    )]);
    fixture.write("profile.json", r#"{"schema":1,"source":["src/lib.rs"],"modules":[],"workflows":[{"name":"run","path":"src/lib.rs","function":"run"},{"name":"other","path":"src/lib.rs","function":"other"}]}"#);
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .args(["flow", "--root"])
        .arg(fixture.path())
        .args([
            "--architecture",
            "profile.json",
            "--detail",
            "control",
            "--out",
            "target/atlas/run.txt",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let directory = fixture.path().join("target/atlas");
    let diagram: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("run.diagram.json")).unwrap())
            .unwrap();
    let folded = diagram["coverage"]["summarized_nodes"].as_array().unwrap();
    assert!(!folded.is_empty());
    assert!(
        folded
            .iter()
            .any(|node| node["id"].as_str().unwrap().starts_with("w0_"))
    );
    assert!(
        folded
            .iter()
            .any(|node| node["id"].as_str().unwrap().starts_with("w1_"))
    );
    assert!(
        folded
            .iter()
            .any(|node| node["label"].as_str().unwrap().contains("propagate ?"))
    );
    let text = std::fs::read_to_string(directory.join("run.txt")).unwrap();
    assert!(text.contains("x = first(&1) ?"));
    assert!(text.contains("residual"));
    assert!(!text.contains("propagate ?"));
}

#[test]
fn compact_unexpanded_control_refuses_oversize_before_folding() {
    let fixture = Fixture::new(&[(
        "src/lib.rs",
        &format!("pub fn run() {{ {} }}", "external();".repeat(601)),
    )]);
    let flow = workflow::read(fixture.path(), "src/lib.rs", "run").unwrap();
    assert!(
        flow.diagram().nodes.len() > 600,
        "detailed view keeps its existing behavior"
    );
    let error = flow
        .diagram_with_detail(workflow::Detail::Control)
        .unwrap_err();
    assert!(error.to_string().contains("600"));
}

#[test]
fn an_expanded_callee_notes_its_local_declarations() {
    let (_f, d) = inspect(
        &[(
            "src/lib.rs",
            "pub fn run(){ use std::mem::swap as exchange; helper(); helper(); }\nfn helper(){ fn inner(){} const LIMIT: u8 = 3; }\n",
        )],
        "run",
        1,
    );
    let declared: Vec<_> = d
        .notes
        .iter()
        .filter(|n| n.contains(" declares "))
        .collect();
    assert_eq!(declared.len(), 3, "{declared:?}");
    assert!(declared[0].starts_with("run declares use std::mem::swap as exchange;"));
    assert!(
        declared
            .iter()
            .any(|n| n.starts_with("helper declares fn inner() at src/lib.rs:2"))
    );
    assert!(
        declared
            .iter()
            .any(|n| n.starts_with("helper declares const LIMIT: u8 = 3;"))
    );
    assert!(
        !d.nodes
            .iter()
            .any(|n| n.label.contains("[opaque]") && n.label.contains("inner"))
    );
}
