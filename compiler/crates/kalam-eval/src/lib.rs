//! `kalam-eval`: the Kalam eval harness (eval/LOOP.md, eval/SCORECARD.md, eval/scorecard.json).
//!
//! It computes every scorecard metric that can be measured with the components that exist
//! today, reports every other metric as `not_measurable` (scored 0, as SCORECARD.md §1
//! requires for `unavailable`), and never turns a partial check into a pass.

pub mod checks;
pub mod corpus;
pub mod history;
pub mod report;
pub mod scorecard;
