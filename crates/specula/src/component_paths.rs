//! Short component aliases → Bevy 0.19 Reflect fully-qualified type paths.
//!
//! Agents often pass `Name` / `Transform`; BRP needs FQNs.

use anyhow::{bail, Result};

/// Bevy 0.19 Reflect path for `Name`.
pub const FQN_NAME: &str = "bevy_ecs::name::Name";
/// Bevy 0.19 Reflect path for `Transform`.
pub const FQN_TRANSFORM: &str = "bevy_transform::components::transform::Transform";
/// Bevy 0.19 Reflect path for `GlobalTransform`.
pub const FQN_GLOBAL_TRANSFORM: &str =
    "bevy_transform::components::global_transform::GlobalTransform";

/// Expand a single component path: aliases → FQN; paths with `::` pass through.
pub fn expand_component_path(raw: &str) -> Result<String> {
    let s = raw.trim();
    if s.is_empty() {
        bail!("empty component path");
    }
    if s.contains("::") {
        return Ok(s.to_string());
    }
    match s {
        "Name" => Ok(FQN_NAME.into()),
        "Transform" => Ok(FQN_TRANSFORM.into()),
        "GlobalTransform" => Ok(FQN_GLOBAL_TRANSFORM.into()),
        other => {
            let list = known_aliases().join(", ");
            bail!(
                "unknown component alias '{other}'. Use a fully-qualified Reflect path, or one of: \
                 {list}. Examples: {FQN_NAME}, {FQN_TRANSFORM}"
            )
        }
    }
}

/// Expand a list of component paths (query).
pub fn expand_component_paths(raw: &[String]) -> Result<Vec<String>> {
    raw.iter().map(|s| expand_component_path(s)).collect()
}

/// Default components for `bevy_brp_query` when the agent omits `components`.
pub fn default_query_components() -> Vec<String> {
    vec!["Name".into(), "Transform".into()]
}

/// Known alias names (for docs/tests).
pub fn known_aliases() -> &'static [&'static str] {
    &["Name", "Transform", "GlobalTransform"]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_name_and_transform() {
        assert_eq!(expand_component_path("Name").unwrap(), FQN_NAME);
        assert_eq!(expand_component_path("Transform").unwrap(), FQN_TRANSFORM);
        assert_eq!(
            expand_component_path("GlobalTransform").unwrap(),
            FQN_GLOBAL_TRANSFORM
        );
    }

    #[test]
    fn passes_through_fqn() {
        assert_eq!(
            expand_component_path(FQN_NAME).unwrap(),
            FQN_NAME
        );
        assert_eq!(
            expand_component_path("bevy_transform::components::transform::Transform").unwrap(),
            FQN_TRANSFORM
        );
    }

    #[test]
    fn rejects_unknown_short_alias() {
        let err = expand_component_path("Player").unwrap_err().to_string();
        assert!(err.contains("unknown component alias"));
        assert!(err.contains("Name"));
        assert!(err.contains("Transform"));
    }

    #[test]
    fn expands_list_and_defaults() {
        let raw = vec!["Name".into(), "Transform".into()];
        let out = expand_component_paths(&raw).unwrap();
        assert_eq!(out, vec![FQN_NAME, FQN_TRANSFORM]);
        let d = expand_component_paths(&default_query_components()).unwrap();
        assert_eq!(d.len(), 2);
        assert!(d[0].contains("Name"));
        assert!(d[1].contains("Transform"));
    }

    #[test]
    fn trims_whitespace() {
        assert_eq!(expand_component_path("  Name  ").unwrap(), FQN_NAME);
    }
}
