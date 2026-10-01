//! The one parameter `task.get` and `task.cancel` take, read leniently.
//!
//! On the REST route the id is in the path, `GET /api/v1/tasks/{id}`, and the
//! REST layer hands it over as `id`. Everywhere else it is `task_id`. A client
//! that sent both, the path and a `?task_id=` as well, used to be refused with
//! "duplicate field `task_id`", because a serde alias counts as the same field
//! twice. Both spellings naming one task is fine; two that disagree is a
//! mistake worth naming.

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct TaskRequest {
    /// The id a long running method answered with. Spelled `id` on the REST
    /// route, where it is in the path, and `task_id` everywhere else, which
    /// is what 03 section 6 calls it.
    #[serde(alias = "id")]
    pub task_id: String,
}

/// Both spellings, each optional, so neither is a duplicate of the other.
#[derive(Deserialize)]
struct Spellings {
    task_id: Option<String>,
    id: Option<String>,
}

impl<'de> Deserialize<'de> for TaskRequest {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let s = Spellings::deserialize(de)?;
        let task_id = match (s.task_id, s.id) {
            (Some(a), Some(b)) if a != b => {
                return Err(D::Error::custom(format!(
                    "`id` '{b}' and `task_id` '{a}' name two different tasks; send one of them"
                )))
            }
            (Some(a), _) | (None, Some(a)) => a,
            (None, None) => return Err(D::Error::missing_field("task_id")),
        };
        Ok(Self { task_id })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn read(v: serde_json::Value) -> Result<TaskRequest, String> {
        serde_json::from_value(v).map_err(|e| e.to_string())
    }

    #[test]
    fn either_spelling_or_both_when_they_agree() {
        assert_eq!(read(json!({"task_id": "t-1"})).unwrap().task_id, "t-1");
        assert_eq!(read(json!({"id": "t-1"})).unwrap().task_id, "t-1");
        assert_eq!(read(json!({"id": "t-1", "task_id": "t-1"})).unwrap().task_id, "t-1");
    }

    #[test]
    fn two_that_disagree_or_none_is_refused_by_name() {
        let e = read(json!({"id": "t-1", "task_id": "t-2"})).unwrap_err();
        assert!(e.contains("two different tasks"), "{e}");
        let e = read(json!({})).unwrap_err();
        assert!(e.contains("task_id"), "{e}");
    }
}
