//! Confidence-based routing for review escalation.

use ferret_core::{Category, PredictedFinding, TeacherKind};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterConfig {
    pub solo_threshold: f32,
    pub verify_threshold: f32,
    pub max_solo_findings: usize,
    #[serde(default)]
    pub category_overrides: Vec<CategoryOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryOverride {
    pub category: Category,
    pub escalate_below: f32,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            solo_threshold: 0.98,
            verify_threshold: 0.75,
            max_solo_findings: 20,
            category_overrides: vec![CategoryOverride {
                category: Category::Architecture,
                escalate_below: 0.95,
            }],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Route {
    Solo,
    SoloWithOptionalVerify,
    Escalate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutedFinding {
    pub finding: PredictedFinding,
    pub route: Route,
    pub escalate_to: Option<TeacherKind>,
}

pub struct Router {
    config: RouterConfig,
    default_teacher: TeacherKind,
}

impl Router {
    pub fn new(config: RouterConfig) -> Self {
        Self {
            config,
            default_teacher: TeacherKind::CodeRabbit,
        }
    }

    pub fn route(&self, findings: Vec<PredictedFinding>) -> Vec<RoutedFinding> {
        findings
            .into_iter()
            .take(self.config.max_solo_findings)
            .map(|finding| self.route_one(finding))
            .collect()
    }

    fn route_one(&self, finding: PredictedFinding) -> RoutedFinding {
        let threshold = self
            .config
            .category_overrides
            .iter()
            .find(|o| o.category == finding.category)
            .map(|o| o.escalate_below)
            .unwrap_or(self.config.verify_threshold);

        let route = if finding.confidence >= self.config.solo_threshold {
            Route::Solo
        } else if finding.confidence >= threshold {
            Route::SoloWithOptionalVerify
        } else {
            Route::Escalate
        };

        RoutedFinding {
            escalate_to: if route == Route::Escalate {
                Some(self.default_teacher)
            } else {
                None
            },
            finding,
            route,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferret_core::Severity;

    fn sample(confidence: f32) -> PredictedFinding {
        PredictedFinding {
            hunk_id: None,
            category: Category::Style,
            severity: Severity::Minor,
            message: "test".into(),
            confidence,
            rationale_refs: vec![],
        }
    }

    #[test]
    fn routes_by_confidence() {
        let router = Router::new(RouterConfig::default());
        let routed = router.route(vec![sample(0.99), sample(0.80), sample(0.50)]);
        assert_eq!(routed[0].route, Route::Solo);
        assert_eq!(routed[1].route, Route::SoloWithOptionalVerify);
        assert_eq!(routed[2].route, Route::Escalate);
    }
}
