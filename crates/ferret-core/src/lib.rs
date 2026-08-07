//! Core domain types for Ferret review distillation.

pub mod error;
pub mod finding;
pub mod outcome;
pub mod repo;
pub mod snapshot;
pub mod teacher;

pub use error::FerretError;
pub use finding::{Category, FindingId, ReviewFinding, Severity};
pub use outcome::{Disposition, Outcome};
pub use repo::{RepoId, RepoProfile};
pub use snapshot::{DiffHunk, PredictedFinding, ReviewSnapshot, SnapshotId};
pub use teacher::{EscalationRequest, EscalationResult, Teacher, TeacherKind};
