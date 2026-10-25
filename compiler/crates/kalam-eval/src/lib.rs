//! `kalam-eval`: the Kalam eval harness (eval/LOOP.md, eval/SCORECARD.md, eval/scorecard.json).
//!
//! It computes every scorecard metric that can be measured with the components that exist
//! today, reports every other metric as `not_measurable` (scored 0, as SCORECARD.md §1
//! requires for `unavailable`), and never turns a partial check into a pass.

pub mod checkpoints;
pub mod checks;
pub mod clauses;
pub mod cli;
pub mod conformance;
pub mod corpus;
pub mod corpus_run;
pub mod diagcat;
pub mod external;
pub mod goldens;
pub mod history;
pub mod holdout;
pub mod integrity;
pub mod judge;
pub mod m2;
pub mod parity;
pub mod ratchet;
pub mod report;
pub mod scorecard;
pub mod suites;
pub mod tiers;
