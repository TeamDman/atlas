//! Opt-in declared wrapper semantics attached to source-resolution observations.
//! Profiles are inputs, not compiler proof; callers must validate their custody.
use super::Site;
use serde::{Deserialize, Serialize};

/// Caller-supplied wrapper semantics and independently obtained source resolutions.
/// Callers must validate source/version custody and the soundness of each contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FutureProfile {
    pub schema: u32,
    /// Optional presentation cut points: only these function bodies expand.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope: Vec<FunctionSelector>,
    /// Optional top-level match arms retained for presentation; no edges added.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<MatchSelection>,
    pub sources: Vec<ProfileSource>,
    pub resolutions: Vec<Resolution>,
    pub contracts: Vec<Contract>,
}
/// One existing function body selected for expansion; this does not add links.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunctionSelector {
    pub path: String,
    pub function: String,
}
/// A written top-level match and one of its exact arm patterns.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchSelection {
    pub path: String,
    pub function: String,
    pub line: usize,
    pub pattern: String,
}
/// Exact source bytes required by the profile's author.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileSource {
    pub path: String,
    pub sha256: String,
}
/// A source call token and its externally observed definition identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resolution {
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub spelling: String,
    pub definition: String,
}
/// The operation a declaration is asserted to perform on a future value.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contract {
    pub definition: String,
    pub operation: Operation,
    pub evidence: String,
}
/// These declarations describe value custody, not proof that any call runs.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Operation {
    /// The returned future forwards polling to the receiver future.
    ReceiverForward,
    /// The returned future forwards polling to this explicit argument.
    ArgumentForward { argument: usize },
    /// The synchronous call may poll this argument and returns normally only
    /// after that future completes. Early failure and no poll remain possible.
    PollArgument { argument: usize },
    /// Polling may first admit and invoke the factory argument; only after a
    /// second cancellation check may its returned future be polled.
    GuardedFactory { argument: usize, arity: usize },
    /// Declares the standard optional container type's identity.
    OptionType,
    /// Declares the corresponding optional container's present-value variant.
    OptionSome,
}
impl FutureProfile {
    pub(in crate::analysis) fn operation(&self, site: &Site, spelling: &str) -> Option<&Contract> {
        if self.schema != 1 {
            return None;
        }
        let word = spelling.rsplit("::").next()?;
        let matches: Vec<_> = self
            .resolutions
            .iter()
            .filter(|r| {
                r.path == site.path
                    && r.spelling == word
                    && (site.line, site.column) <= (r.line, r.column)
                    && (r.line, r.column) <= (site.end_line, site.end_column)
            })
            .collect();
        let [resolution] = matches.as_slice() else {
            return None;
        };
        let contracts: Vec<_> = self
            .contracts
            .iter()
            .filter(|c| c.definition == resolution.definition)
            .collect();
        let [contract] = contracts.as_slice() else {
            return None;
        };
        Some(contract)
    }
}
