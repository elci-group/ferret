//! Teacher adapters for external review sources.

pub mod github;
pub mod sarif;
pub mod stub;

pub use github::GitHubTeacher;
pub use sarif::SarifTeacher;
pub use stub::{CiFixLoopTeacher, CodeRabbitTeacher};
