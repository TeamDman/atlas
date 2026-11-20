//! Executable source witnesses qualify the structural observations, not feasibility inference.
mod support;
use cargo_atlas::{diagram::Diagram, discovery, workflow};
use std::collections::BTreeSet;
use support::Fixture;

#[test]
fn enum_payload_follows_a_unique_public_globbed_crate_alias() {
    use cargo_atlas::workflow::future_profile::{Contract, FutureProfile, Operation, Resolution};
    let app = "pub mod surface;mod carrier;use crate::surface::alias::Arg;enum Cmd{Run(Arg)}impl Cmd{async fn invoke(self){match self{Cmd::Run(args)=>Box::pin(args.invoke()).await}}}struct Shell{command:Cmd}impl Shell{async fn run(self){self.command.invoke().await}}pub async fn entry(){Shell{command:Cmd::Run(domain::Arg)}.run().await;}";
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
        ("app/src/lib.rs", app),
        ("app/src/surface.rs", "pub use crate::carrier::*;"),
        ("app/src/carrier.rs", "pub use domain as alias;"),
        ("app/src/carrier2.rs", "pub use crate::alternate as alias;"),
        (
            "app/src/alternate.rs",
            "pub struct Arg;impl Arg{pub async fn invoke(self){}}",
        ),
        (
            "domain/src/lib.rs",
            "pub struct Arg;impl Arg{pub async fn invoke(self){}}",
        ),
    ]);
    let compiled = std::process::Command::new(env!("CARGO"))
        .current_dir(fixture.path())
        .args(["check", "--offline", "--workspace", "--target-dir"])
        .arg(fixture.path().join("target"))
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let generate = |text: &str| {
        fixture.write("app/src/lib.rs", text);
        let metadata = cargo_metadata::MetadataCommand::new()
            .current_dir(fixture.path())
            .no_deps()
            .exec()
            .unwrap();
        let snapshot = discovery::snapshot_metadata(&metadata).unwrap();
        let analysis = cargo_atlas::analysis::Analysis::new(&snapshot).unwrap();
        let profile = FutureProfile {
            schema: 1,
            scope: vec![],
            routes: vec![],
            sources: vec![],
            resolutions: vec![Resolution {
                path: "app/src/lib.rs".into(),
                line: 1,
                column: text.find("pin(").unwrap(),
                spelling: "pin".into(),
                definition: "fixture::pin".into(),
            }],
            contracts: vec![Contract {
                definition: "fixture::pin".into(),
                operation: Operation::ArgumentForward { argument: 0 },
                evidence: "declared forwarding wrapper".into(),
            }],
        };
        analysis
            .expanded_with_future_profile(
                "app/src/lib.rs",
                "entry",
                3,
                workflow::Detail::Expressions,
                &profile,
            )
            .unwrap()
            .1
    };
    let diagram = generate(app);
    assert!(
        diagram
            .nodes
            .iter()
            .any(|node| node.label == "fn Arg::invoke"),
        "{:?}",
        diagram
            .nodes
            .iter()
            .filter(|node| node.label.contains("invoke"))
            .map(|node| &node.label)
            .collect::<Vec<_>>()
    );
    fixture.write(
        "app/src/surface.rs",
        "pub use crate::carrier::*;pub use crate::carrier2::*;",
    );
    let ambiguous = app.replace(
        "mod carrier;",
        "mod carrier;mod carrier2;pub mod alternate;",
    );
    let diagram = generate(&ambiguous);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|node| node.label == "fn Arg::invoke")
    );
}

#[test]
fn enum_variant_payloads_and_enum_fields_select_written_methods() {
    use cargo_atlas::workflow::future_profile::{Contract, FutureProfile, Operation, Resolution};
    let source = "struct A;struct B;impl A{async fn invoke(self){}}impl B{async fn invoke(self){}}enum Cmd{A(A),B(B)}impl Cmd{async fn invoke(self){match self{Cmd::A(args)=>Box::pin(args.invoke()).await,Cmd::B(args)=>Box::pin(args.invoke()).await}}}struct Shell{command:Cmd}impl Shell{async fn run(self){self.command.invoke().await}}pub async fn entry(){Shell{command:Cmd::A(A)}.run().await;}";
    let make = |text: &str, allow_pin: bool| {
        let fixture = Fixture::new(&[("src/lib.rs", text)]);
        let metadata = cargo_metadata::MetadataCommand::new()
            .current_dir(fixture.path())
            .no_deps()
            .exec()
            .unwrap();
        let snapshot = discovery::snapshot_metadata(&metadata).unwrap();
        let analysis = cargo_atlas::analysis::Analysis::new(&snapshot).unwrap();
        let resolutions = text
            .match_indices("pin(")
            .map(|(column, _)| Resolution {
                path: "src/lib.rs".into(),
                line: 1,
                column,
                spelling: "pin".into(),
                definition: "fixture::pin".into(),
            })
            .collect();
        let contracts = if allow_pin {
            vec![Contract {
                definition: "fixture::pin".into(),
                operation: Operation::ArgumentForward { argument: 0 },
                evidence: "declared forwarding wrapper".into(),
            }]
        } else {
            vec![]
        };
        let profile = FutureProfile {
            schema: 1,
            scope: vec![],
            routes: vec![],
            sources: vec![],
            resolutions,
            contracts,
        };
        (
            analysis
                .expanded_with_future_profile(
                    "src/lib.rs",
                    "entry",
                    3,
                    workflow::Detail::Expressions,
                    &profile,
                )
                .unwrap()
                .1,
            fixture,
        )
    };
    let (diagram, fixture) = make(source, true);
    let compiled = std::process::Command::new(env!("CARGO"))
        .current_dir(fixture.path())
        .args(["check", "--offline", "--target-dir"])
        .arg(fixture.path().join("target"))
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    assert!(
        diagram
            .nodes
            .iter()
            .any(|node| node.label == "fn Cmd::invoke")
    );
    assert!(
        diagram
            .nodes
            .iter()
            .any(|node| node.label == "fn A::invoke")
    );
    assert!(
        diagram
            .nodes
            .iter()
            .any(|node| node.label == "fn B::invoke")
    );
    let (diagram, _fixture) = make(source, false);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|node| node.label == "fn A::invoke" || node.label == "fn B::invoke")
    );
    let shadowed = source.replace(
        "Cmd::A(args)=>Box::pin(args.invoke()).await",
        "Cmd::A(args)=>{let args=B;Box::pin(args.invoke()).await}",
    );
    let (diagram, _fixture) = make(&shadowed, true);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|node| node.label == "fn A::invoke")
    );
    assert!(
        diagram
            .nodes
            .iter()
            .any(|node| node.label == "fn B::invoke")
    );
    let conditional = source.replace(
        "enum Cmd{A(A),",
        "enum Cmd{#[cfg(feature=\"alternate\")] A(A),A(B),",
    );
    let (diagram, _fixture) = make(&conditional, true);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|node| node.label == "fn A::invoke")
    );
}

#[test]
fn profiled_optional_match_and_awaited_wrapper_select_concrete_enum() {
    use cargo_atlas::workflow::future_profile::{Contract, FutureProfile, Operation, Resolution};
    let source = "enum Command{Run}enum Other{Run}impl Command{async fn invoke(self){}}impl Other{async fn invoke(self){}}struct Cli{command:Option<Command>}impl Cli{async fn invoke(self){match self.command{Some(cmd)=>Box::pin(cmd.invoke()).await,None=>()}}}pub async fn entry(){Cli{command:Some(Command::Run)}.invoke().await;}";
    let make = |text: &str, missing: &str| {
        let fixture = Fixture::new(&[("src/lib.rs", text)]);
        let metadata = cargo_metadata::MetadataCommand::new()
            .current_dir(fixture.path())
            .no_deps()
            .exec()
            .unwrap();
        let snapshot = discovery::snapshot_metadata(&metadata).unwrap();
        let analysis = cargo_atlas::analysis::Analysis::new(&snapshot).unwrap();
        let resolutions = [
            ("Option", "Option<", "fixture::option"),
            ("Some", "Some(cmd)", "fixture::some"),
            ("pin", "pin(", "fixture::pin"),
        ]
        .iter()
        .map(|(spelling, needle, definition)| Resolution {
            path: "src/lib.rs".into(),
            line: 1,
            column: text.find(needle).unwrap(),
            spelling: (*spelling).into(),
            definition: (*definition).into(),
        })
        .collect();
        let contracts = [
            ("fixture::option", Operation::OptionType),
            ("fixture::some", Operation::OptionSome),
            ("fixture::pin", Operation::ArgumentForward { argument: 0 }),
        ]
        .into_iter()
        .filter(|(definition, _)| *definition != missing)
        .map(|(definition, operation)| Contract {
            definition: definition.into(),
            operation,
            evidence: "declared source witness".into(),
        })
        .collect();
        let profile = FutureProfile {
            schema: 1,
            routes: vec![],
            scope: vec![],
            sources: vec![],
            resolutions,
            contracts,
        };
        (
            analysis
                .expanded_with_future_profile(
                    "src/lib.rs",
                    "entry",
                    2,
                    workflow::Detail::Expressions,
                    &profile,
                )
                .unwrap()
                .1,
            fixture,
        )
    };
    let (diagram, fixture) = make(source, "");
    let compiled = std::process::Command::new(env!("CARGO"))
        .current_dir(fixture.path())
        .args(["check", "--offline", "--target-dir"])
        .arg(fixture.path().join("target"))
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    assert!(
        diagram
            .nodes
            .iter()
            .any(|node| node.label == "fn Command::invoke")
    );
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|node| node.label == "fn Other::invoke")
    );
    let other = source
        .replace("Option<Command>", "Option<Other>")
        .replace("Some(Command::Run)", "Some(Other::Run)");
    let (diagram, _fixture) = make(&other, "");
    assert!(
        diagram
            .nodes
            .iter()
            .any(|node| node.label == "fn Other::invoke")
    );
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|node| node.label == "fn Command::invoke")
    );
    for missing in ["fixture::option", "fixture::some", "fixture::pin"] {
        let (diagram, _fixture) = make(source, missing);
        assert!(
            !diagram
                .nodes
                .iter()
                .any(|node| node.label == "fn Command::invoke")
        );
    }
    for changed in [
        source.replace("Some(cmd)=>", "Some(cmd) if true=>"),
        source.replace(
            "Some(cmd)=>Box::pin(cmd.invoke()).await",
            "Some(cmd)=>{let cmd=Other::Run;Box::pin(cmd.invoke()).await}",
        ),
        source.replace("Box::pin(cmd.invoke()).await", "Box::pin(cmd.invoke())"),
    ] {
        let (diagram, _fixture) = make(&changed, "");
        assert!(
            !diagram
                .nodes
                .iter()
                .any(|node| node.label == "fn Command::invoke" || node.label == "fn Other::invoke")
        );
    }
}

#[test]
fn callback_receiver_uses_concrete_written_generic_argument_type() {
    let source = "use std::future::Future; struct A; struct B; impl A{async fn run(self){}} impl B{async fn run(self){}} async fn invoke<C,F,T>(c:C,f:F) where F:FnOnce(C)->T,T:Future<Output=()>{f(c).await} pub async fn entry(){let value:A=A;invoke(value,|cli|async move{cli.run().await}).await;}";
    let (fixture, diagram) = inspect(&[("src/lib.rs", source)], "entry", 2);
    let compiled = std::process::Command::new(env!("CARGO"))
        .current_dir(fixture.path())
        .args(["check", "--offline", "--target-dir"])
        .arg(fixture.path().join("target"))
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    assert!(diagram.nodes.iter().any(|node| node.label == "fn A::run"));
    assert!(!diagram.nodes.iter().any(|node| node.label == "fn B::run"));
    assert!(
        diagram
            .nodes
            .iter()
            .any(|node| node.label.contains("explicit caller binding type")
                && node.label.contains("source candidate"))
    );
    let b = source.replace("let value:A=A;", "let value:B=B;");
    let (_, diagram) = inspect(&[("src/lib.rs", &b)], "entry", 2);
    assert!(diagram.nodes.iter().any(|node| node.label == "fn B::run"));
    assert!(!diagram.nodes.iter().any(|node| node.label == "fn A::run"));
    for changed in [
        source.replace("let value:A=A;", "let value=A;"),
        source.replace("let value:A=A;", "let value:A=A;let value:B=B;"),
        source.replace("cli.run().await", "let cli=B;cli.run().await"),
        source.replace("|cli|", "|cli:B|"),
        source.replace("cli.run().await", "cli.run()"),
        source.replace("}).await;}", "});}"),
    ] {
        let (_, diagram) = inspect(&[("src/lib.rs", &changed)], "entry", 2);
        assert!(
            !diagram
                .nodes
                .iter()
                .any(|node| node.label == "fn A::run" || node.label == "fn B::run")
        );
    }
}

#[test]
fn callback_receiver_type_crosses_a_workspace_crate_boundary() {
    let app = "struct A;struct B;impl A{async fn run(self){}}impl B{async fn run(self){}}pub async fn entry(){let value:A=A;domain::invoke(value,|cli|async move{cli.run().await}).await;}";
    let domain = "use std::future::Future;pub async fn invoke<C,F,T>(c:C,f:F) where F:FnOnce(C)->T,T:Future<Output=()>{f(c).await}";
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
        ("app/src/lib.rs", app),
        ("domain/src/lib.rs", domain),
    ]);
    let compiled = std::process::Command::new(env!("CARGO"))
        .current_dir(fixture.path())
        .args(["check", "--offline", "--workspace", "--target-dir"])
        .arg(fixture.path().join("target"))
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let metadata = cargo_metadata::MetadataCommand::new()
        .current_dir(fixture.path())
        .no_deps()
        .exec()
        .unwrap();
    let graph = discovery::scan_metadata(&metadata).unwrap();
    let diagram = workflow::expanded(
        fixture.path(),
        &metadata,
        &graph,
        "app/src/lib.rs",
        "entry",
        2,
    )
    .unwrap()
    .1;
    assert!(diagram.nodes.iter().any(|node| node.label == "fn A::run"));
    assert!(!diagram.nodes.iter().any(|node| node.label == "fn B::run"));
}

#[test]
fn declared_expansion_scope_only_cuts_existing_calls() {
    use cargo_atlas::workflow::future_profile::{FunctionSelector, FutureProfile};
    let fixture = Fixture::new(&[(
        "src/lib.rs",
        "fn leaf(){} fn child(){leaf();} fn other(){} pub fn entry(){child();}",
    )]);
    let metadata = cargo_metadata::MetadataCommand::new()
        .current_dir(fixture.path())
        .no_deps()
        .exec()
        .unwrap();
    let snapshot = discovery::snapshot_metadata(&metadata).unwrap();
    let analysis = cargo_atlas::analysis::Analysis::new(&snapshot).unwrap();
    let mut profile = FutureProfile {
        schema: 1,
        routes: vec![],
        scope: vec![FunctionSelector {
            path: "src/lib.rs".into(),
            function: "child".into(),
        }],
        sources: vec![],
        resolutions: vec![],
        contracts: vec![],
    };
    let diagram = analysis
        .expanded_with_future_profile(
            "src/lib.rs",
            "entry",
            3,
            workflow::Detail::Expressions,
            &profile,
        )
        .unwrap()
        .1;
    assert!(diagram.nodes.iter().any(|n| n.label == "fn child"));
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|n| n.label == "fn leaf" || n.label == "fn other")
    );
    assert!(
        diagram
            .nodes
            .iter()
            .any(|n| n.label.contains("profile cut point"))
    );
    profile.scope[0].function = "missing".into();
    assert!(
        analysis
            .expanded_with_future_profile(
                "src/lib.rs",
                "entry",
                3,
                workflow::Detail::Expressions,
                &profile
            )
            .is_err()
    );
}

#[test]
fn profile_route_selects_an_existing_arm_and_preserves_other_alternatives() {
    use cargo_atlas::workflow::future_profile::{FutureProfile, MatchSelection};
    let fixture = Fixture::new(&[(
        "src/lib.rs",
        "fn hit(){}fn miss(){}fn choose(x:bool){match x{true=>hit(),false=>miss()}}pub fn entry(){choose(true);}",
    )]);
    let metadata = cargo_metadata::MetadataCommand::new()
        .current_dir(fixture.path())
        .no_deps()
        .exec()
        .unwrap();
    let snapshot = discovery::snapshot_metadata(&metadata).unwrap();
    let analysis = cargo_atlas::analysis::Analysis::new(&snapshot).unwrap();
    let mut profile = FutureProfile {
        schema: 1,
        scope: vec![],
        routes: vec![MatchSelection {
            path: "src/lib.rs".into(),
            function: "choose".into(),
            line: 1,
            pattern: "true".into(),
        }],
        sources: vec![],
        resolutions: vec![],
        contracts: vec![],
    };
    let diagram = analysis
        .expanded_with_future_profile(
            "src/lib.rs",
            "entry",
            3,
            workflow::Detail::Expressions,
            &profile,
        )
        .unwrap()
        .1;
    assert!(diagram.nodes.iter().any(|node| node.label == "fn hit"));
    assert!(!diagram.nodes.iter().any(|node| node.label == "fn miss"));
    assert!(
        diagram
            .nodes
            .iter()
            .any(|node| node.label.contains("other match arms [opaque"))
    );
    profile.routes[0].pattern = "not an arm".into();
    assert!(
        analysis
            .expanded_with_future_profile(
                "src/lib.rs",
                "entry",
                3,
                workflow::Detail::Expressions,
                &profile
            )
            .is_err()
    );
}

#[test]
fn profiled_factory_requires_admission_and_a_later_future_poll() {
    use cargo_atlas::workflow::future_profile::{Contract, FutureProfile, Operation, Resolution};
    let source = "use std::{future::Future,pin::Pin,task::{Context,Waker}}; fn wrap<T>(v:T)->T{v} fn drive<F:Future>(f:F){let mut f=Box::pin(f);let mut cx=Context::from_waker(Waker::noop());let _=f.as_mut().poll(&mut cx);} fn reject()->bool{false} fn cancelled()->bool{false} struct Scope; impl Scope { fn first<G,T>(&self,g:G)->Pin<Box<dyn Future<Output=()>>> where G:FnOnce(())->T+'static,T:Future<Output=()>+'static{Box::pin(async move{if reject(){return;}let f=g(());if cancelled(){return;}f.await})} fn second<G,T>(&self,g:G)->Pin<Box<dyn Future<Output=()>>> where G:FnOnce(())->T+'static,T:Future<Output=()>+'static{self.first(g)} } fn bridge<F:Future>(f:F){let f=wrap(f);drive(wrap(f));} fn run<F,T>(invoke:F) where F:FnOnce()->T+'static,T:Future<Output=()>+'static{let scope=Scope;let flag=false;let invocation=wrap(async move{let run_handler=|arg:()|async move{let _=arg;invoke().await};let handler=if flag{scope.first(run_handler)}else{scope.second(run_handler)};handler.await});bridge(invocation);} fn marker(){} pub fn entry(){run(||async{marker();});}";
    let make = |text: &str, missing: bool| {
        let fixture = Fixture::new(&[("src/lib.rs", text)]);
        let metadata = cargo_metadata::MetadataCommand::new()
            .current_dir(fixture.path())
            .no_deps()
            .exec()
            .unwrap();
        let snapshot = discovery::snapshot_metadata(&metadata).unwrap();
        let analysis = cargo_atlas::analysis::Analysis::new(&snapshot).unwrap();
        let mut resolutions = Vec::new();
        for spelling in ["wrap", "drive", "first", "second"] {
            for (column, _) in text.match_indices(&format!("{spelling}(")) {
                resolutions.push(Resolution {
                    path: "src/lib.rs".into(),
                    line: 1,
                    column,
                    spelling: spelling.into(),
                    definition: format!("fixture::{spelling}"),
                });
            }
        }
        let mut contracts = vec![
            Contract {
                definition: "fixture::wrap".into(),
                operation: Operation::ArgumentForward { argument: 0 },
                evidence: "identity".into(),
            },
            Contract {
                definition: "fixture::drive".into(),
                operation: Operation::PollArgument { argument: 0 },
                evidence: "poll".into(),
            },
            Contract {
                definition: "fixture::first".into(),
                operation: Operation::GuardedFactory {
                    argument: 0,
                    arity: 1,
                },
                evidence: "admission and poll".into(),
            },
            Contract {
                definition: "fixture::second".into(),
                operation: Operation::GuardedFactory {
                    argument: 0,
                    arity: 1,
                },
                evidence: "admission and poll".into(),
            },
        ];
        if missing {
            contracts.pop();
        }
        let profile = FutureProfile {
            schema: 1,
            routes: vec![],
            scope: vec![],
            sources: vec![],
            resolutions,
            contracts,
        };
        (
            analysis
                .expanded_with_future_profile(
                    "src/lib.rs",
                    "entry",
                    2,
                    workflow::Detail::Expressions,
                    &profile,
                )
                .unwrap()
                .1,
            fixture,
        )
    };
    let (diagram, fixture) = make(source, false);
    let compiled = std::process::Command::new(env!("CARGO"))
        .current_dir(fixture.path())
        .args(["check", "--offline", "--target-dir"])
        .arg(fixture.path().join("target"))
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let root = diagram
        .nodes
        .iter()
        .find(|node| node.label == "fn entry")
        .unwrap();
    let targets: Vec<_> = diagram
        .nodes
        .iter()
        .filter(|node| node.label.starts_with("[opaque] marker()"))
        .collect();
    assert_eq!(targets.len(), 2);
    assert!(
        targets
            .iter()
            .all(|node| reachable(&diagram, &root.id, &node.id))
    );
    let gates: Vec<_> = diagram
        .nodes
        .iter()
        .filter(|node| node.label.contains("scope factory admitted and invoked"))
        .collect();
    assert_eq!(gates.len(), 2);
    for gate in gates {
        assert!(
            targets
                .iter()
                .any(|target| reachable(&diagram, &gate.id, &target.id))
        );
    }
    for node in diagram.nodes.iter().filter(|node| {
        node.label.contains("scope not admitted")
            || node.label.contains("cancelled before factory future poll")
            || node.label.contains("scope future not polled")
    }) {
        assert!(
            targets
                .iter()
                .all(|target| !reachable(&diagram, &node.id, &target.id))
        );
    }
    for (text, missing) in [
        (source.to_owned(), true),
        (
            source.replace("bridge(invocation);", "drop(invocation);"),
            false,
        ),
        (
            source.replace("let run_handler=", "let invoke=||async{};let run_handler="),
            false,
        ),
        (source.replace("invoke().await", "async {}.await"), false),
    ] {
        let (diagram, _fixture) = make(&text, missing);
        assert!(
            !diagram
                .nodes
                .iter()
                .any(|node| node.label.starts_with("[opaque] marker()"))
        );
    }
}

#[test]
fn profiled_future_custody_requires_forwarders_poll_and_unshadowed_values() {
    use cargo_atlas::workflow::future_profile::{Contract, FutureProfile, Operation, Resolution};
    let source = "use std::future::Future; use std::task::{Context,Waker}; fn wrap<T>(v:T)->T{v} fn drive<F:Future>(f:F){let mut f=Box::pin(f);let w=Waker::noop();let mut cx=Context::from_waker(w);let _=f.as_mut().poll(&mut cx);} fn blocked()->bool{false} fn bridge<F:Future>(f:F){ if blocked(){return;} let f=wrap(f); drive(wrap(f)); } fn marker(){} pub fn entry(){let f=wrap(async {marker();});bridge(f);}";
    let make = |text: &str, mode: &str| {
        let fixture = Fixture::new(&[("src/lib.rs", text)]);
        let metadata = cargo_metadata::MetadataCommand::new()
            .current_dir(fixture.path())
            .no_deps()
            .exec()
            .unwrap();
        let snapshot = discovery::snapshot_metadata(&metadata).unwrap();
        let analysis = cargo_atlas::analysis::Analysis::new(&snapshot).unwrap();
        let mut resolutions = Vec::new();
        for (spelling, definition) in [("wrap", "fixture::wrap"), ("drive", "fixture::drive")] {
            for (column, _) in text.match_indices(&format!("{spelling}(")) {
                resolutions.push(Resolution {
                    path: "src/lib.rs".into(),
                    line: 1,
                    column,
                    spelling: spelling.into(),
                    definition: definition.into(),
                });
            }
        }
        let mut profile = FutureProfile {
            schema: 1,
            routes: vec![],
            scope: vec![],
            sources: vec![],
            resolutions,
            contracts: vec![
                Contract {
                    definition: "fixture::wrap".into(),
                    operation: Operation::ArgumentForward { argument: 0 },
                    evidence: "written identity function".into(),
                },
                Contract {
                    definition: "fixture::drive".into(),
                    operation: Operation::PollArgument { argument: 0 },
                    evidence: "written Future::poll".into(),
                },
            ],
        };
        match mode {
            "missing-wrapper" => profile
                .contracts
                .retain(|contract| contract.definition != "fixture::wrap"),
            "missing-poll" => profile
                .contracts
                .retain(|contract| contract.definition != "fixture::drive"),
            "wrong-symbol" => profile
                .resolutions
                .iter_mut()
                .filter(|resolution| resolution.spelling == "drive")
                .for_each(|resolution| resolution.definition = "other::drive".into()),
            "ambiguous" => {
                let duplicate = profile.resolutions.first().unwrap().clone();
                profile.resolutions.push(duplicate);
            }
            _ => {}
        }
        (
            analysis
                .expanded_with_future_profile(
                    "src/lib.rs",
                    "entry",
                    2,
                    workflow::Detail::Expressions,
                    &profile,
                )
                .unwrap()
                .1,
            fixture,
        )
    };
    let (diagram, fixture) = make(source, "valid");
    let compiled = std::process::Command::new(env!("CARGO"))
        .current_dir(fixture.path())
        .args(["check", "--offline", "--target-dir"])
        .arg(fixture.path().join("target"))
        .output()
        .expect("compile source fixture");
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let root = diagram
        .nodes
        .iter()
        .find(|n| n.label == "fn entry")
        .unwrap();
    let poll = diagram
        .nodes
        .iter()
        .find(|n| n.label.contains("poll forwarded future"))
        .unwrap();
    let body = diagram
        .nodes
        .iter()
        .find(|n| n.label.starts_with("[opaque] marker()"))
        .unwrap();
    let early = diagram.nodes.iter().find(|n| n.label == "return").unwrap();
    assert!(reachable(&diagram, &root.id, &body.id));
    assert!(!reachable_without(&diagram, &root.id, &body.id, &poll.id));
    assert!(!reachable(&diagram, &early.id, &body.id));
    for changed in [
        source.replace("bridge(f);", "drop(f);"),
        source.replace("let f=wrap(f);", "if blocked(){let f=();} let f=wrap(f);"),
    ] {
        let (diagram, _fixture) = make(&changed, "valid");
        assert!(
            !diagram
                .nodes
                .iter()
                .any(|n| n.label.contains("poll forwarded future"))
        );
    }
    for mode in [
        "missing-wrapper",
        "missing-poll",
        "wrong-symbol",
        "ambiguous",
    ] {
        let (diagram, _fixture) = make(source, mode);
        assert!(
            !diagram
                .nodes
                .iter()
                .any(|n| n.label.contains("poll forwarded future")),
            "{mode}"
        );
    }
}

#[test]
fn builder_chain_uses_written_self_returns_and_preserves_uncertainty() {
    let library = "pub struct App; impl App { pub fn new()->Self{Self} pub fn first(self)->Self{self} pub fn second(self)->Self{self} pub fn third(self)->Self{self} pub fn run(self){} }";
    let entry = "mod library; use library::App; pub fn entry(){ let app=App::new().first().second(); app.third().run(); }";
    let (_, diagram) = inspect(
        &[("src/lib.rs", entry), ("src/library.rs", library)],
        "entry",
        2,
    );
    assert!(diagram.nodes.iter().any(|node| node.label == "fn App::run"));
    assert!(diagram.nodes.iter().any(|node| {
        node.label
            .contains("receiver follows uniquely written constructor")
            && node.label.contains("source candidate")
    }));
    let conditional = library.replace("pub fn second", "#[cfg(feature = \"on\")] pub fn second");
    let (_, diagram) = inspect(
        &[("src/lib.rs", entry), ("src/library.rs", &conditional)],
        "entry",
        2,
    );
    assert!(diagram.nodes.iter().any(|node| node.label == "fn App::run"));
    assert!(diagram.nodes.iter().any(|node| {
        node.label
            .contains("conditional on #[cfg(feature = \"on\")]")
    }));
    let competing = format!("{library} impl App {{ pub fn second(self)->Self{{self}} }}");
    let (_, diagram) = inspect(
        &[("src/lib.rs", entry), ("src/library.rs", &competing)],
        "entry",
        2,
    );
    assert!(!diagram.nodes.iter().any(|node| node.label == "fn App::run"));
    let unknown = library.replace("fn second(self)->Self", "fn second(self)->Other");
    let (_, diagram) = inspect(
        &[("src/lib.rs", entry), ("src/library.rs", &unknown)],
        "entry",
        2,
    );
    assert!(!diagram.nodes.iter().any(|node| node.label == "fn App::run"));
    let overwritten = entry.replace("app.third().run();", "app = unknown(); app.third().run();");
    let (_, diagram) = inspect(
        &[("src/lib.rs", &overwritten), ("src/library.rs", library)],
        "entry",
        2,
    );
    assert!(!diagram.nodes.iter().any(|node| node.label == "fn App::run"));
    let local_shadow = entry.replace("let app=", "struct App; let app=");
    let (_, diagram) = inspect(
        &[("src/lib.rs", &local_shadow), ("src/library.rs", library)],
        "entry",
        2,
    );
    assert!(!diagram.nodes.iter().any(|node| node.label == "fn App::run"));
}

#[test]
fn captured_callback_in_a_polled_nested_closure_keeps_its_origin() {
    let source = "use std::future::Future; async fn inner<G,GF>(factory:G) where G:FnOnce()->GF, GF:Future<Output=()> { factory().await; } async fn outer<F,FF>(invoke:F) where F:FnOnce()->FF, FF:Future<Output=()> { inner(|| async move { invoke().await; }).await; } pub async fn entry(){ outer(|| async { finish(); }).await; } fn finish(){}";
    let (_, diagram) = inspect(&[("src/lib.rs", source)], "entry", 4);
    assert_eq!(
        diagram
            .nodes
            .iter()
            .filter(|n| n.label.contains("poll callback's async future"))
            .count(),
        2
    );
    assert!(
        diagram
            .nodes
            .iter()
            .any(|n| n.label.starts_with("[opaque] finish()"))
    );
    let unpolled = source.replace(
        "inner(|| async move { invoke().await; }).await;",
        "drop(inner(|| async move { invoke().await; }));",
    );
    let (_, diagram) = inspect(&[("src/lib.rs", &unpolled)], "entry", 4);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|n| n.label.starts_with("[opaque] finish()"))
    );
    let shadowed = source.replace(
        "invoke().await; }).await;",
        "let invoke = || async {}; invoke().await; }).await;",
    );
    let (_, diagram) = inspect(&[("src/lib.rs", &shadowed)], "entry", 4);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|n| n.label.starts_with("[opaque] finish()"))
    );
    let ignored = source.replace("factory().await;", "drop(factory);");
    let (_, diagram) = inspect(&[("src/lib.rs", &ignored)], "entry", 4);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|n| n.label.starts_with("[opaque] finish()"))
    );
    let overwritten = source.replace(
        "invoke().await; }).await;",
        "invoke = other; invoke().await; }).await;",
    );
    let (_, diagram) = inspect(&[("src/lib.rs", &overwritten)], "entry", 4);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|n| n.label.starts_with("[opaque] finish()"))
    );
}

#[test]
fn captured_callback_crosses_a_crate_only_after_admission_and_poll() {
    let app = "use std::future::Future; async fn outer<F,FF>(invoke:F) where F:FnOnce()->FF, FF:Future<Output=()> { domain::inner(|| async move { invoke().await; }).await; } pub async fn entry(){ outer(|| async { finish(); }).await; } fn finish(){}";
    let domain = "use std::future::Future; pub async fn inner<G,GF>(factory:G) where G:FnOnce()->GF, GF:Future<Output=()> { if cancelled(){return;} factory().await; } fn cancelled()->bool{false}";
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
        ("app/src/lib.rs", app),
        ("domain/src/lib.rs", domain),
    ]);
    let compiled = std::process::Command::new(env!("CARGO"))
        .current_dir(fixture.path())
        .args(["check", "--offline", "--workspace", "--target-dir"])
        .arg(fixture.path().join("target"))
        .output()
        .expect("compile fixture");
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let metadata = cargo_metadata::MetadataCommand::new()
        .current_dir(fixture.path())
        .no_deps()
        .exec()
        .expect("metadata");
    let graph = discovery::scan_metadata(&metadata).expect("discovery");
    let (_, diagram) = workflow::expanded(
        fixture.path(),
        &metadata,
        &graph,
        "app/src/lib.rs",
        "entry",
        4,
    )
    .expect("flow");
    let finish = diagram
        .nodes
        .iter()
        .find(|n| n.label.starts_with("[opaque] finish()"))
        .expect("nested callback body");
    let rejection = diagram
        .nodes
        .iter()
        .find(|n| n.label == "return")
        .expect("early cancellation return");
    assert!(!reachable(&diagram, &rejection.id, &finish.id));
    let entry = diagram
        .nodes
        .iter()
        .find(|n| n.label == "fn entry")
        .expect("entry");
    let gates: Vec<_> = diagram
        .nodes
        .iter()
        .filter(|n| n.label.contains("poll callback's async future"))
        .collect();
    assert_eq!(gates.len(), 2);
    for gate in gates {
        assert!(!reachable_without(
            &diagram, &entry.id, &finish.id, &gate.id
        ));
    }
}

#[test]
fn builder_callback_inside_returned_future_stays_opaque() {
    let source = "use std::future::Future; struct App; impl App { fn new()->Self{Self} fn setup(self)->Self{self} fn run<F, Fut>(self, invoke:F) where F:FnOnce()->Fut, Fut:Future<Output=()> { let future=Box::pin(async move { invoke().await }); wait(future); } } async fn handle(){} fn wait<T>(_future:T){} pub fn entry(){App::new().setup().run(|| async {handle().await});}";
    let (_, diagram) = inspect(&[("src/lib.rs", source)], "entry", 2);
    assert!(diagram.nodes.iter().any(|node| node.label == "fn App::run"));
    assert!(
        diagram
            .nodes
            .iter()
            .any(|node| node.label.contains("opaque") && node.label.contains("async move"))
    );
    assert!(!diagram.nodes.iter().any(|node| node.label == "fn handle"));
}

#[test]
fn directly_awaited_generic_callback_enters_only_its_supplied_async_body() {
    let source = "use std::future::Future; fn cancelled()->bool{false} async fn invoke<F,Fut>(callback:F) where F:FnOnce()->Fut, Fut:Future<Output=()> { if cancelled(){return;} callback().await; } async fn page(){} pub async fn entry(){ invoke(|| async { page().await; }).await; }";
    let (_, diagram) = inspect(&[("src/lib.rs", source)], "entry", 2);
    let poll = diagram
        .nodes
        .iter()
        .find(|node| node.label.contains("poll callback's async future"))
        .expect("callback poll gate");
    assert!(
        diagram.edges.iter().any(
            |edge| edge.to == poll.id && edge.label.as_deref() == Some("if invoked and polled")
        )
    );
    assert!(
        diagram
            .nodes
            .iter()
            .any(|node| node.label.contains("page().await"))
    );
    let ret = diagram
        .nodes
        .iter()
        .find(|node| node.label == "return")
        .expect("cancellation return");
    assert!(!reachable(&diagram, &ret.id, &poll.id));
    let dropped = source.replace(
        "invoke(|| async { page().await; }).await;",
        "invoke(|| async { page().await; });",
    );
    let (_, diagram) = inspect(&[("src/lib.rs", &dropped)], "entry", 2);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|node| node.label.contains("poll callback's async future"))
    );
    let ignored = source.replace("callback().await;", "drop(callback);");
    let (_, diagram) = inspect(&[("src/lib.rs", &ignored)], "entry", 2);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|node| node.label.contains("poll callback's async future"))
    );
    let shadowed = source.replace(
        "callback().await;",
        "let callback = other; callback().await;",
    );
    let (_, diagram) = inspect(&[("src/lib.rs", &shadowed)], "entry", 2);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|node| node.label.contains("poll callback's async future"))
    );
    let unrelated = source.replace(
        "invoke(|| async { page().await; }).await;",
        "invoke(other).await; let unused = || async { page().await; };",
    );
    let (_, diagram) = inspect(&[("src/lib.rs", &unrelated)], "entry", 2);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|node| node.label.contains("poll callback's async future"))
    );
    let wrong_argument = source
        .replace(
            "invoke<F,Fut>(callback:F)",
            "invoke<F,Fut,G>(callback:F, unused:G)",
        )
        .replace(
            "invoke(|| async { page().await; }).await;",
            "invoke(other, || async { page().await; }).await;",
        );
    let (_, diagram) = inspect(&[("src/lib.rs", &wrong_argument)], "entry", 2);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|node| node.label.contains("poll callback's async future"))
    );
    let different_call = source.replace("callback().await;", "other().await;");
    let (_, diagram) = inspect(&[("src/lib.rs", &different_call)], "entry", 2);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|node| node.label.contains("poll callback's async future"))
    );
    let nonstandard_bound = source.replace("F:FnOnce()", "F:fake::FnOnce()");
    let (_, diagram) = inspect(&[("src/lib.rs", &nonstandard_bound)], "entry", 2);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|node| node.label.contains("poll callback's async future"))
    );
    let shadowed_trait = format!("trait FnOnce {{}} {source}");
    let (_, diagram) = inspect(&[("src/lib.rs", &shadowed_trait)], "entry", 2);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|node| node.label.contains("poll callback's async future"))
    );
    let async_closure = source.replace(
        "invoke(|| async { page().await; })",
        "invoke(async || async { page().await; })",
    );
    let (_, diagram) = inspect(&[("src/lib.rs", &async_closure)], "entry", 2);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|node| node.label.contains("poll callback's async future"))
    );
    let callback_return = source.replace("page().await;", "return;");
    let (_, diagram) = inspect(&[("src/lib.rs", &callback_return)], "entry", 2);
    let end = diagram
        .nodes
        .iter()
        .find(|node| node.label == "callback async future completes (value or residual)")
        .expect("callback exit");
    assert!(
        diagram
            .nodes
            .iter()
            .filter(|node| node.label == "return")
            .any(|node| diagram.edges.iter().any(|edge| edge.from == node.id
                && edge.to == end.id
                && edge.label.as_deref() == Some("transfer")))
    );
}

#[test]
fn directly_awaited_callback_residual_exits_its_future() {
    let source = "use std::future::Future; fn fallible()->Result<(),()>{Ok(())} async fn invoke<F,Fut>(callback:F)->Result<(),()> where F:FnOnce()->Fut, Fut:Future<Output=Result<(),()>> { callback().await } pub async fn entry()->Result<(),()>{ invoke(|| async { fallible()?; Ok(()) }).await }";
    let (_, diagram) = inspect(&[("src/lib.rs", source)], "entry", 2);
    let end = diagram
        .nodes
        .iter()
        .find(|node| node.label == "callback async future completes (value or residual)")
        .expect("callback exit");
    let residual = diagram
        .nodes
        .iter()
        .find(|node| node.label == "? returns early (residual)")
        .expect("callback residual");
    assert!(diagram.edges.iter().any(|edge| edge.from == residual.id
        && edge.to == end.id
        && edge.label.as_deref() == Some("return residual")));
}

#[test]
fn concrete_await_substitution_selects_generic_field_trait_across_crates() {
    let fixture = Fixture::new(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers=['app','domain','model']\nresolver='3'\n",
        ),
        (
            "app/Cargo.toml",
            "[package]\nname='app'\nversion='0.1.0'\nedition='2024'\n[dependencies]\ndomain={path='../domain'}\nmodel={path='../model'}\n",
        ),
        (
            "domain/Cargo.toml",
            "[package]\nname='domain'\nversion='0.1.0'\nedition='2024'\n",
        ),
        (
            "model/Cargo.toml",
            "[package]\nname='model'\nversion='0.1.0'\nedition='2024'\n[dependencies]\ndomain={path='../domain'}\n",
        ),
        (
            "app/src/lib.rs",
            "use domain::Execute; use model::Req; pub async fn entry(){ Req{}.execute().await; }",
        ),
        (
            "domain/src/lib.rs",
            "use std::future::{IntoFuture,Ready,ready}; pub struct Work<R>{request:R} pub trait Execute:Sized { fn run(self); fn execute(self)->Work<Self>{Work{request:self}} } fn cancelled()->bool{false} impl<R:Execute> Work<R> { fn drive(self)->Ready<()> { if cancelled(){return ready(());} self.request.run(); ready(()) } } impl<R:Execute> IntoFuture for Work<R>{type Output=();type IntoFuture=Ready<()>;fn into_future(self)->Self::IntoFuture{self.drive()}}",
        ),
        (
            "model/src/lib.rs",
            "use domain::Execute; pub struct Req{} pub struct Other{} impl Execute for Req { fn run(self){} } impl Execute for Other { fn run(self){} }",
        ),
    ]);
    let generate = || {
        let metadata = cargo_metadata::MetadataCommand::new()
            .current_dir(fixture.path())
            .no_deps()
            .exec()
            .expect("fixture metadata");
        let graph = discovery::scan_metadata(&metadata).expect("fixture discovery");
        workflow::expanded(
            fixture.path(),
            &metadata,
            &graph,
            "app/src/lib.rs",
            "entry",
            4,
        )
        .expect("fixture flow")
        .1
    };
    let compiled = std::process::Command::new(env!("CARGO"))
        .current_dir(fixture.path())
        .args(["check", "--offline", "--workspace", "--target-dir"])
        .arg(fixture.path().join("target"))
        .output()
        .expect("run fixture compiler");
    assert!(
        compiled.status.success(),
        "fixture compile: {}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let diagram = generate();
    let chosen = diagram
        .nodes
        .iter()
        .find(|node| node.label.contains("fn Req::run"))
        .expect("concrete request implementation");
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|node| node.label.contains("fn Other::run"))
    );
    let cancelled = diagram
        .nodes
        .iter()
        .find(|node| node.label.starts_with("return ready"))
        .expect("cancellation return");
    assert!(!reachable(&diagram, &cancelled.id, &chosen.id));
    assert!(
        diagram
            .nodes
            .iter()
            .any(|node| node.label.contains("source candidate")
                && node.label.contains("Execute via impl"))
    );
    fixture.write(
        "app/src/lib.rs",
        "use domain::Execute; use model::Req; pub async fn entry(){ Req{}.execute(); }",
    );
    assert!(
        !generate()
            .nodes
            .iter()
            .any(|node| node.label.contains("fn Req::run"))
    );
    fixture.write(
        "app/src/lib.rs",
        "use domain::Execute; use model::Other; pub async fn entry(){ Other{}.execute().await; }",
    );
    let other = generate();
    assert!(
        other
            .nodes
            .iter()
            .any(|node| node.label.contains("fn Other::run"))
    );
    assert!(
        !other
            .nodes
            .iter()
            .any(|node| node.label.contains("fn Req::run"))
    );
    let domain =
        std::fs::read_to_string(fixture.path().join("domain/src/lib.rs")).expect("fixture source");
    let model =
        std::fs::read_to_string(fixture.path().join("model/src/lib.rs")).expect("fixture source");
    fixture.write(
        "app/src/lib.rs",
        "use domain::Execute; use model::Req; pub async fn entry(){ Req{}.execute().await; }",
    );
    fixture.write(
        "model/src/lib.rs",
        &format!("{model}\nimpl Req {{ pub fn run(self) {{}} }}"),
    );
    let trait_dispatch = generate();
    let selected = trait_dispatch
        .nodes
        .iter()
        .find(|node| node.label.contains("fn Req::run"))
        .expect("trait dispatch");
    assert!(
        selected
            .source_sites
            .iter()
            .any(|site| site.ends_with("model/src/lib.rs:1"))
    );
    fixture.write("model/src/lib.rs", &model);
    fixture.write(
        "domain/src/lib.rs",
        &domain.replace(
            "impl<R:Execute> Work<R>",
            "trait Clash { fn run(self); } impl<R:Execute+Clash> Work<R>",
        ),
    );
    assert!(
        !generate()
            .nodes
            .iter()
            .any(|node| node.label.contains("fn Req::run"))
    );
    fixture.write(
        "domain/src/lib.rs",
        &domain.replace(
            "impl<R:Execute> Work<R>",
            "trait GenericClash<T> { fn run(self); } impl<R:Execute+GenericClash<u8>> Work<R>",
        ),
    );
    assert!(
        !generate()
            .nodes
            .iter()
            .any(|node| node.label.contains("fn Req::run"))
    );
    fixture.write("domain/src/lib.rs", &domain);
    fixture.write(
        "model/src/lib.rs",
        &format!(
            "{model} #[cfg(feature=\"alternate\")] impl Execute for Req {{ fn run(self){{}} }}"
        ),
    );
    assert!(
        !generate()
            .nodes
            .iter()
            .any(|node| node.label.contains("fn Req::run"))
    );
    fixture.write("domain/src/lib.rs", "use std::future::{IntoFuture,Ready,ready}; pub struct Work<R>{request:R} pub mod api { use super::Work; pub trait Execute:Sized { fn run(self); fn execute(self)->Work<Self>{Work{request:self}} } } impl<R:api::Execute> Work<R> { fn drive(self)->Ready<()> { self.request.run(); ready(()) } } impl<R:api::Execute> IntoFuture for Work<R>{type Output=();type IntoFuture=Ready<()>;fn into_future(self)->Self::IntoFuture{self.drive()}}");
    fixture.write(
        "model/src/lib.rs",
        "use domain::api::Execute; pub struct Req{} impl Execute for Req { fn run(self){} }",
    );
    fixture.write(
        "app/src/lib.rs",
        "use domain::api::Execute; use model::Req; pub async fn entry(){ Req{}.execute().await; }",
    );
    assert!(
        generate()
            .nodes
            .iter()
            .any(|node| node.label.contains("fn Req::run"))
    );
}

#[test]
fn pinned_returned_async_method_preserves_concrete_cross_crate_substitution() {
    let domain = "use std::{future::{Future,IntoFuture},pin::Pin}; pub struct Req{} pub struct Other{} pub struct Work<R>{request:R} pub trait Execute:Sized { fn run(self); fn execute(self)->Work<Self>{Work{request:self}} } impl Execute for Req { fn run(self){} } impl Execute for Other { fn run(self){} } fn cancelled()->bool{false} impl<R:Execute+'static> Work<R> { async fn drive(self) { if cancelled(){return;} self.request.run(); } } impl<R:Execute+'static> IntoFuture for Work<R>{type Output=();type IntoFuture=Pin<Box<dyn Future<Output=()>>>;fn into_future(self)->Self::IntoFuture{::std::boxed::Box::pin(self.drive())}}";
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
            "use domain::{Req, Execute}; pub async fn entry(){ Req{}.execute().await; }",
        ),
        ("domain/src/lib.rs", domain),
    ]);
    let compiled = std::process::Command::new(env!("CARGO"))
        .current_dir(fixture.path())
        .args(["check", "--offline", "--workspace", "--target-dir"])
        .arg(fixture.path().join("target"))
        .output()
        .expect("run fixture compiler");
    assert!(
        compiled.status.success(),
        "fixture compile: {}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let generate = || {
        let metadata = cargo_metadata::MetadataCommand::new()
            .current_dir(fixture.path())
            .no_deps()
            .exec()
            .expect("fixture metadata");
        let graph = discovery::scan_metadata(&metadata).expect("fixture discovery");
        workflow::expanded(
            fixture.path(),
            &metadata,
            &graph,
            "app/src/lib.rs",
            "entry",
            5,
        )
        .expect("fixture flow")
        .1
    };
    let diagram = generate();
    let poll = diagram
        .nodes
        .iter()
        .find(|node| {
            node.label
                .contains("poll returned pinned future [source candidate")
        })
        .expect("pinned future poll");
    let body = diagram
        .nodes
        .iter()
        .find(|node| node.label.contains("fn Work<R>::drive"))
        .expect("polled async body");
    let chosen = diagram
        .nodes
        .iter()
        .find(|node| node.label.contains("fn Req::run"))
        .expect("concrete request body");
    assert!(reachable(&diagram, &poll.id, &body.id) && reachable(&diagram, &body.id, &chosen.id));
    let root = diagram
        .nodes
        .iter()
        .find(|node| node.label == "fn entry")
        .expect("entry node");
    assert!(!reachable_without(&diagram, &root.id, &chosen.id, &poll.id));
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|node| node.label.contains("fn Other::run"))
    );
    let cancelled = diagram
        .nodes
        .iter()
        .find(|node| node.label == "return")
        .expect("cancellation return");
    assert!(!reachable(&diagram, &cancelled.id, &chosen.id));
    fixture.write(
        "app/src/lib.rs",
        "use domain::{Req, Execute}; pub async fn entry(){ Req{}.execute(); }",
    );
    assert!(
        !generate()
            .nodes
            .iter()
            .any(|node| node.label.contains("fn Req::run"))
    );
    fixture.write(
        "app/src/lib.rs",
        "use domain::{Other, Execute}; pub async fn entry(){ Other{}.execute().await; }",
    );
    let other = generate();
    assert!(
        other
            .nodes
            .iter()
            .any(|node| node.label.contains("fn Other::run"))
    );
    assert!(
        !other
            .nodes
            .iter()
            .any(|node| node.label.contains("fn Req::run"))
    );
    fixture.write(
        "domain/src/lib.rs",
        &domain.replace(
            "::std::boxed::Box::pin(self.drive())",
            "{ let _never_polled = self.drive(); ::std::boxed::Box::pin(async {}) }",
        ),
    );
    assert!(
        !generate()
            .nodes
            .iter()
            .any(|node| node.label.contains("fn Other::run"))
    );
    fixture.write(
        "domain/src/lib.rs",
        &domain.replace("::std::boxed::Box::pin", "custom::Box::pin"),
    );
    assert!(
        !generate()
            .nodes
            .iter()
            .any(|node| node.label.contains("fn Other::run"))
    );
    fixture.write(
        "domain/src/lib.rs",
        &format!("use crate::fake as std; {domain}"),
    );
    assert!(
        !generate()
            .nodes
            .iter()
            .any(|node| node.label.contains("fn Other::run"))
    );
}

#[test]
fn async_function_body_is_not_expanded_at_future_construction() {
    let (_, unpolled) = inspect(
        &[(
            "src/lib.rs",
            "async fn page(){work();} fn work(){} pub fn entry(){page();}",
        )],
        "entry",
        2,
    );
    assert!(
        !unpolled
            .nodes
            .iter()
            .any(|node| node.label == "fn page" || node.label == "fn work")
    );
    let (_, polled) = inspect(
        &[(
            "src/lib.rs",
            "async fn page(){work();} fn work(){} pub async fn entry(){page().await;}",
        )],
        "entry",
        2,
    );
    assert!(polled.nodes.iter().any(|node| node.label == "fn page"));
    assert!(
        polled
            .edges
            .iter()
            .any(|edge| edge.label.as_deref() == Some("if polled"))
    );
}

#[test]
fn awaited_callback_requires_matching_written_argument_arity() {
    let source = "use std::future::Future; async fn invoke<F,Fut>(callback:F) where F:FnOnce(u8,u8)->Fut, Fut:Future<Output=()> { callback(1,2).await; } async fn page(_:u8,_:u8){} pub async fn entry(){ invoke(|cli, context| async move { page(cli,context).await; }).await; }";
    let (_, diagram) = inspect(&[("src/lib.rs", source)], "entry", 2);
    assert!(
        diagram
            .nodes
            .iter()
            .any(|node| node.label.contains("poll callback's async future"))
    );
    assert!(
        diagram
            .nodes
            .iter()
            .any(|node| node.label.contains("page(cli,context).await"))
    );
    let wrong_invoke = source.replace("callback(1,2).await", "callback(1).await");
    let (_, diagram) = inspect(&[("src/lib.rs", &wrong_invoke)], "entry", 2);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|node| node.label.contains("poll callback's async future"))
    );
    let wrong_closure = source.replace("|cli, context|", "|cli|");
    let (_, diagram) = inspect(&[("src/lib.rs", &wrong_closure)], "entry", 2);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|node| node.label.contains("poll callback's async future"))
    );
    let unsupported_pattern = source.replace("|cli, context|", "|(cli, context)|");
    let (_, diagram) = inspect(&[("src/lib.rs", &unsupported_pattern)], "entry", 2);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|node| node.label.contains("poll callback's async future"))
    );
    let competing_bounds = source.replace(
        "F:FnOnce(u8,u8)->Fut",
        "F:FnOnce(u8)->Fut + FnOnce(u8,u8)->Fut",
    );
    let (_, diagram) = inspect(&[("src/lib.rs", &competing_bounds)], "entry", 2);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|node| node.label.contains("poll callback's async future"))
    );
}

#[test]
fn synchronous_callback_match_is_construction_only_without_poll_proof() {
    let source = "use std::future::Future; fn invoke<F,Fut>(value:u8, callback:F) where F:FnOnce(u8,u8)->Fut, Fut:Future<Output=()> { drop((value,callback)); } pub fn entry(){ invoke(1, |cli, context| async move { consume(cli,context); }); } fn consume(_:u8,_:u8){}";
    let (_, diagram) = inspect(&[("src/lib.rs", source)], "entry", 2);
    assert!(diagram.nodes.iter().any(|node| {
        node.label
            .contains("callback argument 2 matches FnOnce parameter callback with 2 arguments")
            && node
                .label
                .contains("invocation and polling not established")
    }));
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|node| node.label.contains("poll callback's async future"))
    );
    assert!(!diagram.nodes.iter().any(|node| node.label == "fn consume"));
    let wrong_arity = source.replace("|cli, context|", "|cli|");
    let (_, diagram) = inspect(&[("src/lib.rs", &wrong_arity)], "entry", 2);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|node| node.label.contains("written callback argument"))
    );
}

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
fn reachable_without(d: &Diagram, from: &str, to: &str, excluded: &str) -> bool {
    let mut todo = vec![from.to_owned()];
    let mut seen = BTreeSet::new();
    while let Some(node) = todo.pop() {
        if node == excluded {
            continue;
        }
        if node == to {
            return true;
        }
        if seen.insert(node.clone()) {
            todo.extend(
                d.edges
                    .iter()
                    .filter(|edge| edge.from == node)
                    .map(|edge| edge.to.clone()),
            );
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
        e.label.as_deref() == Some("if polled")
            && diagram
                .nodes
                .iter()
                .any(|n| n.id == e.from && n.label.contains("self.page::<u8>().await"))
            && diagram
                .nodes
                .iter()
                .any(|n| n.id == e.to && n.label.contains("poll returned async function future"))
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
        e.label.as_deref() == Some("if polled")
            && diagram
                .nodes
                .iter()
                .any(|n| n.id == e.from && n.label.contains("page().await"))
            && diagram
                .nodes
                .iter()
                .any(|n| n.id == e.to && n.label.contains("poll returned async function future"))
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
fn await_follows_unique_written_into_future_candidate_with_concrete_self() {
    let source = "use std::future::IntoFuture; struct Req {} struct Wrap<T>(T); fn helper() {} impl<T> Wrap<T> { fn drive(self) -> std::future::Ready<()> { std::future::ready(()) } } trait Exec: Sized { fn execute(self) -> Wrap<Self> { Wrap(self) } } impl Exec for Req {} impl<T> IntoFuture for Wrap<T> { type Output=(); type IntoFuture=std::future::Ready<()>; fn into_future(self) -> Self::IntoFuture { helper(); self.drive() } } pub async fn run(){ Req {}.execute().await; }";
    let (_, diagram) = inspect(&[("src/lib.rs", source)], "run", 3);
    assert!(
        diagram
            .nodes
            .iter()
            .any(|n| n.label.contains("fn Wrap<T>::into_future"))
    );
    assert!(
        diagram
            .nodes
            .iter()
            .any(|n| n.label.contains("fn Wrap<T>::drive")
                && n.label.contains("source substitution T = fixture::Req"))
    );
    assert!(diagram.nodes.iter().any(|n| n.label == "fn helper"));
    assert!(
        diagram
            .nodes
            .iter()
            .any(|n| n.label.contains("T = fixture::Req")
                && n.label.contains("bounds")
                && n.label.contains("not proved"))
    );
    assert!(diagram.edges.iter().any(|e| {
        e.label.as_deref() == Some("source candidate")
            && diagram
                .nodes
                .iter()
                .any(|n| n.id == e.from && n.label.contains(".await"))
    }));
    let poll = diagram
        .nodes
        .iter()
        .find(|n| n.label.contains("poll returned future [opaque"))
        .unwrap();
    assert!(
        diagram
            .edges
            .iter()
            .any(|e| e.to == poll.id && e.label.as_deref() == Some("candidate returns future"))
    );
    assert!(
        diagram
            .edges
            .iter()
            .any(|e| e.from == poll.id && e.label.as_deref() == Some("if polling completes"))
    );
}

#[test]
fn await_refuses_unpolled_ambiguous_or_unrelated_into_future() {
    let prefix = "use std::future::IntoFuture; struct Req {} struct Wrap<T>(T); trait Exec: Sized { fn execute(self) -> Wrap<Self> { Wrap(self) } } impl Exec for Req {} impl<T> IntoFuture for Wrap<T> { type Output=(); type IntoFuture=std::future::Ready<()>; fn into_future(self) -> Self::IntoFuture { std::future::ready(()) } } ";
    let unawaited = format!("{prefix} pub async fn run(){{ Req {{}}.execute(); }}");
    let (_, diagram) = inspect(&[("src/lib.rs", &unawaited)], "run", 3);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|n| n.label.contains("fn Wrap<T>::into_future"))
    );
    let ambiguous = format!(
        "{prefix} #[cfg(feature=\"other\")] impl IntoFuture for Wrap<Req> {{ type Output=(); type IntoFuture=std::future::Ready<()>; fn into_future(self) -> Self::IntoFuture {{ std::future::ready(()) }} }} pub async fn run(){{ Req {{}}.execute().await; }}"
    );
    let (_, diagram) = inspect(&[("src/lib.rs", &ambiguous)], "run", 3);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|n| n.label.contains("fn Wrap<T>::into_future"))
    );
    assert!(diagram.nodes.iter().any(|n| {
        n.label
            .contains("implicit IntoFuture not uniquely evidenced")
    }));
    let unrelated = "trait IntoFuture { fn into_future(self); } struct Req {} struct Wrap<T>(T); trait Exec: Sized { fn execute(self) -> Wrap<Self> { Wrap(self) } } impl Exec for Req {} impl<T> IntoFuture for Wrap<T> { fn into_future(self) {} } pub async fn run(){ Req {}.execute().await; }";
    let (_, diagram) = inspect(&[("src/lib.rs", unrelated)], "run", 3);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|n| n.label.contains("fn Wrap<T>::into_future"))
    );
    let unmatched = "use std::future::IntoFuture; struct Req {} struct Wrap<T>(T); trait Other {} trait Exec: Sized { fn execute(self) -> Wrap<Self> { Wrap(self) } } impl Exec for Req {} impl<T: Other> IntoFuture for Wrap<T> { type Output=(); type IntoFuture=std::future::Ready<()>; fn into_future(self) -> Self::IntoFuture { std::future::ready(()) } } pub async fn run(){ Req {}.execute().await; }";
    let (_, diagram) = inspect(&[("src/lib.rs", unmatched)], "run", 3);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|n| n.label.contains("fn Wrap<T>::into_future"))
    );
    assert!(diagram.nodes.iter().any(|n| {
        n.label
            .contains("implicit IntoFuture not uniquely evidenced")
    }));
}

#[test]
fn into_future_accepts_only_witnessed_trait_bounds() {
    let source = "use std::future::IntoFuture; struct Req {} struct Wrap<T>(T); trait Exec: Sized { fn execute(self) -> Wrap<Self> { Wrap(self) } } impl Exec for Req {} impl<T: Exec> IntoFuture for Wrap<T> { type Output=(); type IntoFuture=std::future::Ready<()>; fn into_future(self) -> Self::IntoFuture { std::future::ready(()) } } pub async fn run(){ Req {}.execute().await; }";
    let (_, diagram) = inspect(&[("src/lib.rs", source)], "run", 3);
    assert!(
        diagram
            .nodes
            .iter()
            .any(|n| n.label.contains("fn Wrap<T>::into_future"))
    );
    assert!(
        diagram
            .nodes
            .iter()
            .any(|n| n.label.contains("written trait bounds matched"))
    );
    let unsupported = source.replace(
        "impl<T: Exec> IntoFuture",
        "impl<T: Exec + Send> IntoFuture",
    );
    let (_, diagram) = inspect(&[("src/lib.rs", &unsupported)], "run", 3);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|n| n.label.contains("fn Wrap<T>::into_future"))
    );
    let associated = source
        .replace("trait Exec: Sized {", "trait Exec: Sized { type Item;")
        .replace(
            "impl Exec for Req {}",
            "impl Exec for Req { type Item = (); }",
        )
        .replace(
            "impl<T: Exec> IntoFuture for Wrap<T> {",
            "impl<T: Exec> IntoFuture for Wrap<T> where T::Item: 'static {",
        );
    let (_, diagram) = inspect(&[("src/lib.rs", &associated)], "run", 3);
    assert!(
        diagram
            .nodes
            .iter()
            .any(|n| n.label.contains("fn Wrap<T>::into_future"))
    );
    let absent = associated.replace("type Item = ();", "");
    let (_, diagram) = inspect(&[("src/lib.rs", &absent)], "run", 3);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|n| n.label.contains("fn Wrap<T>::into_future"))
    );
}

#[test]
fn into_future_recognizes_explicit_aliases_but_refuses_unknown_glob_competitors() {
    let alias = "use std::future::IntoFuture as Convert; struct Req {} struct Wrap<T>(T); trait Exec: Sized { fn execute(self) -> Wrap<Self> { Wrap(self) } } impl Exec for Req {} impl<T> Convert for Wrap<T> { type Output=(); type IntoFuture=std::future::Ready<()>; fn into_future(self) -> Self::IntoFuture { std::future::ready(()) } } pub async fn run(){ Req {}.execute().await; }";
    let (_, diagram) = inspect(&[("src/lib.rs", alias)], "run", 3);
    assert!(
        diagram
            .nodes
            .iter()
            .any(|n| n.label.contains("fn Wrap<T>::into_future"))
    );
    let ambiguous = "struct Req {} struct Wrap<T>(T); trait Exec: Sized { fn execute(self) -> Wrap<Self> { Wrap(self) } } impl Exec for Req {} mod first { use super::Wrap; use std::future::IntoFuture; impl<T> IntoFuture for Wrap<T> { type Output=(); type IntoFuture=std::future::Ready<()>; fn into_future(self) -> Self::IntoFuture { std::future::ready(()) } } } mod second { use super::{Req,Wrap}; use std::future::*; #[cfg(feature=\"other\")] impl IntoFuture for Wrap<Req> { type Output=(); type IntoFuture=std::future::Ready<()>; fn into_future(self) -> Self::IntoFuture { std::future::ready(()) } } } pub async fn run(){ Req {}.execute().await; }";
    let (_, diagram) = inspect(&[("src/lib.rs", ambiguous)], "run", 3);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|n| n.label.contains("fn Wrap<T>::into_future"))
    );
    assert!(diagram.nodes.iter().any(|n| {
        n.label
            .contains("implicit IntoFuture not uniquely evidenced")
    }));
    let conflicting_future = format!(
        "{alias} #[cfg(feature=\"other\")] impl<T> std::future::Future for Wrap<T> {{ type Output=(); fn poll(self: std::pin::Pin<&mut Self>, _: &mut std::task::Context<'_>) -> std::task::Poll<()> {{ std::task::Poll::Ready(()) }} }}"
    );
    let (_, diagram) = inspect(&[("src/lib.rs", &conflicting_future)], "run", 3);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|n| n.label.contains("fn Wrap<T>::into_future"))
    );
    assert!(diagram.nodes.iter().any(|n| {
        n.label
            .contains("implicit IntoFuture not uniquely evidenced")
    }));
    let spoofed = "mod std { pub mod future { pub trait IntoFuture { fn into_future(self); } } } use std::future::IntoFuture; struct Req {} struct Wrap<T>(T); trait Exec: Sized { fn execute(self) -> Wrap<Self> { Wrap(self) } } impl Exec for Req {} impl<T> IntoFuture for Wrap<T> { fn into_future(self) {} } pub async fn run(){ Req {}.execute().await; }";
    let (_, diagram) = inspect(&[("src/lib.rs", spoofed)], "run", 3);
    assert!(
        !diagram
            .nodes
            .iter()
            .any(|n| n.label.contains("fn Wrap<T>::into_future"))
    );
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

#[test]
fn returned_guarded_future_requires_profile_and_poll_admission() {
    use cargo_atlas::workflow::future_profile::{Contract, FutureProfile, Operation, Resolution};
    let source = include_str!("fixtures/returned_guarded_future.rs");
    let fixture = Fixture::new(&[("src/lib.rs", source)]);
    let compiled = std::process::Command::new(env!("CARGO"))
        .current_dir(fixture.path())
        .args(["check", "--offline", "--target-dir"])
        .arg(fixture.path().join("target"))
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let metadata = cargo_metadata::MetadataCommand::new()
        .current_dir(fixture.path())
        .no_deps()
        .exec()
        .unwrap();
    let snapshot = discovery::snapshot_metadata(&metadata).unwrap();
    let analysis = cargo_atlas::analysis::Analysis::new(&snapshot).unwrap();
    let line = source
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains("Box::pin(Scope.guarded"))
        .unwrap();
    let mut profile = FutureProfile {
        schema: 1,
        scope: vec![],
        routes: vec![],
        sources: vec![],
        resolutions: ["pin", "guarded", "instrument"]
            .iter()
            .map(|spelling| Resolution {
                path: "src/lib.rs".into(),
                line: line.0 + 1,
                column: line.1.find(spelling).unwrap(),
                spelling: (*spelling).into(),
                definition: format!("fixture::{spelling}"),
            })
            .collect(),
        contracts: vec![
            Contract {
                definition: "fixture::pin".into(),
                operation: Operation::ArgumentForward { argument: 0 },
                evidence: "fixture pin".into(),
            },
            Contract {
                definition: "fixture::instrument".into(),
                operation: Operation::ReceiverForward,
                evidence: "fixture instrumentation".into(),
            },
            Contract {
                definition: "fixture::guarded".into(),
                operation: Operation::GuardedFactory {
                    argument: 0,
                    arity: 1,
                },
                evidence: "fixture admission and poll".into(),
            },
        ],
    };
    let generate = |profile: &FutureProfile| {
        analysis
            .expanded_with_future_profile(
                "src/lib.rs",
                "entry",
                6,
                workflow::Detail::Expressions,
                profile,
            )
            .unwrap()
            .1
    };
    let diagram = generate(&profile);
    let root = diagram
        .nodes
        .iter()
        .find(|node| node.label == "fn entry")
        .unwrap();
    let marker = diagram
        .nodes
        .iter()
        .find(|node| {
            node.label.contains("marker()") && node.label.contains("async-body call not indexed")
        })
        .expect("factory body");
    let poll = diagram
        .nodes
        .iter()
        .find(|node| node.label.contains("poll admitted factory future"))
        .expect("conditional poll");
    assert!(reachable(&diagram, &root.id, &marker.id));
    assert!(!reachable_without(&diagram, &root.id, &marker.id, &poll.id));
    for boundary in [
        "scope future unpolled, interrupted or not admitted",
        "cancelled before factory future poll",
    ] {
        let node = diagram
            .nodes
            .iter()
            .find(|node| node.label.contains(boundary))
            .unwrap();
        assert!(!reachable(&diagram, &node.id, &marker.id));
    }
    let complete = profile.clone();
    profile.contracts.pop();
    let missing = generate(&profile);
    assert!(
        !missing
            .nodes
            .iter()
            .any(|node| node.label.contains("marker()")
                && node.label.contains("async-body call not indexed"))
    );
    fixture.write(
        "src/lib.rs",
        &source.replace("Request {}.execute().await", "Request {}.execute()"),
    );
    let snapshot = discovery::snapshot_metadata(&metadata).unwrap();
    let unpolled_analysis = cargo_atlas::analysis::Analysis::new(&snapshot).unwrap();
    let unpolled = unpolled_analysis
        .expanded_with_future_profile(
            "src/lib.rs",
            "entry",
            6,
            workflow::Detail::Expressions,
            &complete,
        )
        .unwrap()
        .1;
    assert!(
        !unpolled
            .nodes
            .iter()
            .any(|node| node.label.contains("poll admitted factory future"))
    );
    fixture.write(
        "src/lib.rs",
        &source.replace(
            "self.run()",
            "if reject() { return Box::pin(async {}); } self.run()",
        ),
    );
    let snapshot = discovery::snapshot_metadata(&metadata).unwrap();
    let early_analysis = cargo_atlas::analysis::Analysis::new(&snapshot).unwrap();
    let early = early_analysis
        .expanded_with_future_profile(
            "src/lib.rs",
            "entry",
            6,
            workflow::Detail::Expressions,
            &complete,
        )
        .unwrap()
        .1;
    assert!(
        !early
            .nodes
            .iter()
            .any(|node| node.label.contains("poll admitted factory future"))
    );
}

#[test]
fn returned_boxed_future_retains_an_explicit_poll_boundary() {
    let source = include_str!("fixtures/returned_future_flow.rs");
    let (_, diagram) = inspect(&[("src/lib.rs", source)], "entry", 4);
    assert!(
        diagram
            .nodes
            .iter()
            .any(|node| node.label.contains("fn RequestExecution<T>::run"))
    );
    assert!(
        diagram
            .nodes
            .iter()
            .any(|node| node.label.contains("poll returned future [opaque"))
    );
    assert!(!diagram.nodes.iter().any(|node| node.label == "fn page"));
    assert!(
        diagram
            .nodes
            .iter()
            .any(|node| node.label.contains("async move") && node.label.contains("opaque"))
    );
}

#[test]
fn guarded_instrumented_future_callbacks_remain_opaque_without_poll_proof() {
    let source = include_str!("fixtures/guarded_future_flow.rs");
    let (_, diagram) = inspect(&[("src/lib.rs", source)], "entry", 5);
    assert!(
        diagram
            .nodes
            .iter()
            .any(|node| node.label.contains("fn RequestExecution<T>::run"))
    );
    assert!(
        diagram
            .nodes
            .iter()
            .any(|node| node.label.contains("poll returned future [opaque"))
    );
    assert!(!diagram.nodes.iter().any(|node| node.label == "fn page"));
    assert!(
        diagram
            .nodes
            .iter()
            .any(|node| node.label.contains("move || async move") && node.label.contains("opaque"))
    );
}

#[test]
fn guarded_fixture_distinguishes_callback_invocation_from_future_polling() {
    use std::{
        cell::Cell,
        future::Future,
        rc::Rc,
        task::{Context, Waker},
    };
    async fn guarded<F, Fut>(admitted: bool, cancelled: bool, factory: F)
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = ()>,
    {
        if !admitted {
            return;
        }
        let future = factory();
        if cancelled {
            return;
        }
        future.await;
    }
    let invoked = Rc::new(Cell::new(0));
    let polled = Rc::new(Cell::new(0));
    let make_factory = || {
        let invoked = Rc::clone(&invoked);
        let polled = Rc::clone(&polled);
        move || {
            invoked.set(invoked.get() + 1);
            async move {
                polled.set(polled.get() + 1);
            }
        }
    };
    drop(guarded(true, false, make_factory()));
    assert_eq!((invoked.get(), polled.get()), (0, 0));
    let mut context = Context::from_waker(Waker::noop());
    assert!(
        Box::pin(guarded(false, false, make_factory()))
            .as_mut()
            .poll(&mut context)
            .is_ready()
    );
    assert_eq!((invoked.get(), polled.get()), (0, 0));
    assert!(
        Box::pin(guarded(true, true, make_factory()))
            .as_mut()
            .poll(&mut context)
            .is_ready()
    );
    assert_eq!((invoked.get(), polled.get()), (1, 0));
    assert!(
        Box::pin(guarded(true, false, make_factory()))
            .as_mut()
            .poll(&mut context)
            .is_ready()
    );
    assert_eq!((invoked.get(), polled.get()), (2, 1));
}
