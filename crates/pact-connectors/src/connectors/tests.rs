// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! Unit tests for the connector registry and trait implementations.

#[cfg(test)]
mod registry {
    use crate::connectors::*;

    #[test]
    fn all_connectors_registered() {
        let registry = ConnectorRegistry::new();
        let names: Vec<&str> = registry.all().iter().map(|c| c.name()).collect();
        for expected in [
            "github", "figma", "slack", "resend", "gdrive", "mermaid", "jira", "notion", "linear",
            "teams", "airtable",
        ] {
            assert!(names.contains(&expected), "missing connector: {expected}");
        }
    }

    #[test]
    fn get_returns_correct_connector() {
        let registry = ConnectorRegistry::new();
        assert_eq!(registry.get("github").unwrap().name(), "github");
        assert_eq!(registry.get("slack").unwrap().name(), "slack");
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn is_known() {
        let registry = ConnectorRegistry::new();
        assert!(registry.is_known("github"));
        assert!(registry.is_known("airtable"));
        assert!(!registry.is_known("nonexistent"));
    }

    #[test]
    fn canonical_name() {
        let registry = ConnectorRegistry::new();
        assert_eq!(registry.canonical_name("github"), "github");
        assert_eq!(registry.canonical_name("slack"), "slack");
    }

    #[test]
    fn full_spec_structure() {
        let registry = ConnectorRegistry::new();
        let spec = registry.full_spec();
        assert_eq!(spec["version"], "1.0.0");
        let connectors = spec["connectors"].as_object().unwrap();
        assert!(connectors.contains_key("github"));
        assert!(connectors.contains_key("notion"));
        assert!(connectors.contains_key("linear"));
        assert!(connectors.contains_key("teams"));
        assert!(connectors.contains_key("airtable"));
    }

    #[test]
    fn all_operations_prefixed() {
        let registry = ConnectorRegistry::new();
        let ops = registry.all_operations();
        assert!(ops.contains(&"github.push_file".to_string()));
        assert!(ops.contains(&"slack.post_message".to_string()));
        assert!(ops.contains(&"notion.search".to_string()));
        assert!(ops.contains(&"linear.create_issue".to_string()));
        assert!(ops.contains(&"teams.post_message".to_string()));
        assert!(ops.contains(&"airtable.list_records".to_string()));
        // Every operation must be dot-separated
        for op in &ops {
            assert!(op.contains('.'), "operation missing dot separator: {op}");
        }
    }

    #[test]
    fn schema_connectors_has_all() {
        let registry = ConnectorRegistry::new();
        let schema = registry.schema_connectors();
        let obj = schema.as_object().unwrap();
        for c in registry.all() {
            assert!(obj.contains_key(c.name()), "schema_connectors missing: {}", c.name());
            let entry = &obj[c.name()];
            assert!(entry["description"].is_string());
            assert!(entry["operations"].is_array());
        }
    }
}

#[cfg(test)]
mod config {
    use crate::connectors::*;

    #[test]
    fn empty_config_deserializes() {
        let config: ConnectorConfig = serde_json::from_str("{}").unwrap();
        assert!(config.github.is_none());
        assert!(config.slack.is_none());
        assert!(config.notion.is_none());
    }

    #[test]
    fn github_config_deserializes() {
        let json = r#"{"github": {"token": "ghp_xxx", "owner": "me", "repo": "myrepo"}}"#;
        let config: ConnectorConfig = serde_json::from_str(json).unwrap();
        let gh = config.github.unwrap();
        assert_eq!(gh.token, "ghp_xxx");
        assert_eq!(gh.owner, "me");
        assert_eq!(gh.repo, "myrepo");
    }

    #[test]
    fn resend_from_defaults() {
        let json = r#"{"resend": {"api_key": "re_xxx"}}"#;
        let config: ConnectorConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.resend.unwrap().from, "onboarding@resend.dev");
    }

    #[test]
    fn airtable_config_with_base_id() {
        let json = r#"{"airtable": {"token": "pat_xxx", "base_id": "appXXXXXX"}}"#;
        let config: ConnectorConfig = serde_json::from_str(json).unwrap();
        let at = config.airtable.unwrap();
        assert_eq!(at.token, "pat_xxx");
        assert_eq!(at.base_id.unwrap(), "appXXXXXX");
    }

    #[test]
    fn airtable_config_without_base_id() {
        let json = r#"{"airtable": {"token": "pat_xxx"}}"#;
        let config: ConnectorConfig = serde_json::from_str(json).unwrap();
        assert!(config.airtable.unwrap().base_id.is_none());
    }

    #[test]
    fn linear_config_deserializes() {
        let json = r#"{"linear": {"api_key": "lin_xxx"}}"#;
        let config: ConnectorConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.linear.unwrap().api_key, "lin_xxx");
    }

    #[test]
    fn teams_config_deserializes() {
        let json = r#"{"teams": {"access_token": "eyJ..."}}"#;
        let config: ConnectorConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.teams.unwrap().access_token, "eyJ...");
    }

    #[test]
    fn notion_config_deserializes() {
        let json = r#"{"notion": {"token": "ntn_xxx"}}"#;
        let config: ConnectorConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.notion.unwrap().token, "ntn_xxx");
    }

    #[test]
    fn multiple_connectors_deserialize() {
        let json = r#"{
            "github": {"token": "ghp_xxx", "owner": "o", "repo": "r"},
            "slack": {"token": "xoxb-xxx"},
            "notion": {"token": "ntn_xxx"}
        }"#;
        let config: ConnectorConfig = serde_json::from_str(json).unwrap();
        assert!(config.github.is_some());
        assert!(config.slack.is_some());
        assert!(config.notion.is_some());
        assert!(config.figma.is_none());
    }
}

#[cfg(test)]
mod is_configured {
    use crate::connectors::*;

    fn empty_config() -> ConnectorConfig {
        ConnectorConfig::default()
    }

    #[test]
    fn not_configured_when_empty() {
        let registry = ConnectorRegistry::new();
        let config = empty_config();
        for c in registry.all() {
            assert!(
                !c.is_configured(&config),
                "{} should not be configured with empty config",
                c.name()
            );
        }
    }

    #[test]
    fn github_configured_when_present() {
        let config = ConnectorConfig {
            github: Some(GitHubConfig {
                token: "t".into(),
                owner: "o".into(),
                repo: "r".into(),
            }),
            ..Default::default()
        };
        let registry = ConnectorRegistry::new();
        assert!(registry.is_configured("github", &config));
        assert!(!registry.is_configured("slack", &config));
    }

    #[test]
    fn notion_configured_when_present() {
        let config = ConnectorConfig {
            notion: Some(NotionConfig {
                token: "ntn_xxx".into(),
            }),
            ..Default::default()
        };
        let registry = ConnectorRegistry::new();
        assert!(registry.is_configured("notion", &config));
        assert!(!registry.is_configured("linear", &config));
    }

    #[test]
    fn all_connectors_respond_to_is_configured() {
        let registry = ConnectorRegistry::new();
        let config = ConnectorConfig {
            github: Some(GitHubConfig { token: "t".into(), owner: "o".into(), repo: "r".into() }),
            figma: Some(FigmaConfig { token: "t".into() }),
            slack: Some(SlackConfig { token: "t".into(), default_channel: None }),
            resend: Some(ResendConfig { api_key: "k".into(), from: "a@b.c".into() }),
            gdrive: Some(GDriveConfig { access_token: "t".into(), default_folder: None }),
            mermaid: Some(MermaidConfig { token: "t".into(), default_project_id: None }),
            jira: Some(JiraConfig { email: "e".into(), api_token: "t".into(), domain: "d".into(), default_project: None }),
            notion: Some(NotionConfig { token: "t".into() }),
            linear: Some(LinearConfig { api_key: "k".into() }),
            teams: Some(TeamsConfig { access_token: "t".into() }),
            airtable: Some(AirtableConfig { token: "t".into(), base_id: None }),
        };
        for c in registry.all() {
            assert!(
                c.is_configured(&config),
                "{} should be configured when its config is provided",
                c.name()
            );
        }
    }
}

#[cfg(test)]
mod trait_impls {
    use crate::connectors::*;

    #[test]
    fn each_connector_has_nonempty_name() {
        let registry = ConnectorRegistry::new();
        for c in registry.all() {
            assert!(!c.name().is_empty());
        }
    }

    #[test]
    fn each_connector_has_nonempty_description() {
        let registry = ConnectorRegistry::new();
        for c in registry.all() {
            assert!(!c.description().is_empty(), "{} has empty description", c.name());
        }
    }

    #[test]
    fn each_connector_has_operations() {
        let registry = ConnectorRegistry::new();
        for c in registry.all() {
            assert!(!c.operations().is_empty(), "{} has no operations", c.name());
        }
    }

    #[test]
    fn each_spec_has_required_fields() {
        let registry = ConnectorRegistry::new();
        for c in registry.all() {
            let spec = c.spec();
            assert!(spec["name"].is_string(), "{} spec missing name", c.name());
            assert_eq!(spec["name"].as_str().unwrap(), c.name());
            assert!(spec["description"].is_string(), "{} spec missing description", c.name());
            assert!(spec["operations"].is_object(), "{} spec missing operations", c.name());
            assert!(spec["credentials"].is_object(), "{} spec missing credentials", c.name());
        }
    }

    #[test]
    fn spec_operations_match_operations_method() {
        let registry = ConnectorRegistry::new();
        for c in registry.all() {
            let spec = c.spec();
            let spec_ops: Vec<String> = spec["operations"]
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect();
            let trait_ops: Vec<String> = c.operations().iter().map(|s| s.to_string()).collect();
            for op in &trait_ops {
                assert!(
                    spec_ops.contains(op),
                    "{}: operation '{}' in operations() but not in spec()",
                    c.name(),
                    op
                );
            }
            for op in &spec_ops {
                assert!(
                    trait_ops.contains(op),
                    "{}: operation '{}' in spec() but not in operations()",
                    c.name(),
                    op
                );
            }
        }
    }

    #[test]
    fn credential_schema_has_name_and_credentials() {
        let registry = ConnectorRegistry::new();
        for c in registry.all() {
            let schema = c.credential_schema();
            assert!(schema["name"].is_string(), "{} cred schema missing name", c.name());
            assert!(schema["credentials"].is_object(), "{} cred schema missing credentials", c.name());
        }
    }

    #[test]
    fn only_mermaid_has_prompt_additions() {
        let registry = ConnectorRegistry::new();
        for c in registry.all() {
            if c.name() == "mermaid" {
                assert!(c.prompt_additions().is_some(), "mermaid should have prompt_additions");
            } else {
                assert!(c.prompt_additions().is_none(), "{} should not have prompt_additions", c.name());
            }
        }
    }
}

#[cfg(test)]
mod param_validation {
    use std::collections::HashMap;
    use crate::connectors::*;

    fn config_with_github() -> ConnectorConfig {
        ConnectorConfig {
            github: Some(GitHubConfig { token: "t".into(), owner: "o".into(), repo: "r".into() }),
            ..Default::default()
        }
    }

    fn config_with_slack() -> ConnectorConfig {
        ConnectorConfig {
            slack: Some(SlackConfig { token: "t".into(), default_channel: None }),
            ..Default::default()
        }
    }

    fn config_with_airtable() -> ConnectorConfig {
        ConnectorConfig {
            airtable: Some(AirtableConfig { token: "t".into(), base_id: None }),
            ..Default::default()
        }
    }

    fn config_with_linear() -> ConnectorConfig {
        ConnectorConfig {
            linear: Some(LinearConfig { api_key: "k".into() }),
            ..Default::default()
        }
    }

    fn config_with_notion() -> ConnectorConfig {
        ConnectorConfig {
            notion: Some(NotionConfig { token: "t".into() }),
            ..Default::default()
        }
    }

    fn config_with_teams() -> ConnectorConfig {
        ConnectorConfig {
            teams: Some(TeamsConfig { access_token: "t".into() }),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn not_configured_error() {
        let registry = ConnectorRegistry::new();
        let result = registry
            .execute_operation("github.push_file", &HashMap::new(), &ConnectorConfig::default())
            .await;
        assert!(matches!(result, Err(ConnectorError::NotConfigured(_))));
    }

    #[tokio::test]
    async fn unknown_connector_error() {
        let registry = ConnectorRegistry::new();
        let result = registry
            .execute_operation("nonexistent.foo", &HashMap::new(), &ConnectorConfig::default())
            .await;
        assert!(matches!(result, Err(ConnectorError::UnknownConnector(_))));
    }

    #[tokio::test]
    async fn invalid_operation_format() {
        let registry = ConnectorRegistry::new();
        let result = registry
            .execute_operation("no_dot", &HashMap::new(), &ConnectorConfig::default())
            .await;
        assert!(matches!(result, Err(ConnectorError::InvalidOperation(_))));
    }

    #[tokio::test]
    async fn github_invalid_action() {
        let connector = github::GitHubConnector;
        let result = connector
            .execute("nonexistent_action", &HashMap::new(), &config_with_github())
            .await;
        assert!(matches!(result, Err(ConnectorError::InvalidOperation(_))));
    }

    #[tokio::test]
    async fn github_push_file_missing_path() {
        let connector = github::GitHubConnector;
        let params: HashMap<String, String> = HashMap::new();
        let result = connector.execute("push_file", &params, &config_with_github()).await;
        assert!(matches!(result, Err(ConnectorError::MissingParam(_))));
    }

    #[tokio::test]
    async fn slack_post_message_missing_channel() {
        let connector = slack::SlackConnector;
        let params: HashMap<String, String> = HashMap::new();
        let result = connector.execute("post_message", &params, &config_with_slack()).await;
        assert!(matches!(result, Err(ConnectorError::MissingParam(_))));
    }

    #[tokio::test]
    async fn slack_invalid_action() {
        let connector = slack::SlackConnector;
        let result = connector
            .execute("nonexistent", &HashMap::new(), &config_with_slack())
            .await;
        assert!(matches!(result, Err(ConnectorError::InvalidOperation(_))));
    }

    #[tokio::test]
    async fn airtable_list_records_missing_table() {
        let connector = airtable::AirtableConnector;
        // base_id not in config and not in params → missing base_id first
        let params: HashMap<String, String> = HashMap::new();
        let result = connector
            .execute("list_records", &params, &config_with_airtable())
            .await;
        assert!(matches!(result, Err(ConnectorError::MissingParam(_))));
    }

    #[tokio::test]
    async fn airtable_invalid_action() {
        let connector = airtable::AirtableConnector;
        let result = connector
            .execute("nonexistent", &HashMap::new(), &config_with_airtable())
            .await;
        assert!(matches!(result, Err(ConnectorError::InvalidOperation(_))));
    }

    #[tokio::test]
    async fn linear_invalid_action() {
        let connector = linear::LinearConnector;
        let result = connector
            .execute("nonexistent", &HashMap::new(), &config_with_linear())
            .await;
        assert!(matches!(result, Err(ConnectorError::InvalidOperation(_))));
    }

    #[tokio::test]
    async fn linear_create_issue_missing_title() {
        let connector = linear::LinearConnector;
        let params: HashMap<String, String> = HashMap::new();
        let result = connector.execute("create_issue", &params, &config_with_linear()).await;
        assert!(matches!(result, Err(ConnectorError::MissingParam(_))));
    }

    #[tokio::test]
    async fn notion_invalid_action() {
        let connector = notion::NotionConnector;
        let result = connector
            .execute("nonexistent", &HashMap::new(), &config_with_notion())
            .await;
        assert!(matches!(result, Err(ConnectorError::InvalidOperation(_))));
    }

    #[tokio::test]
    async fn notion_get_page_missing_page_id() {
        let connector = notion::NotionConnector;
        let params: HashMap<String, String> = HashMap::new();
        let result = connector.execute("get_page", &params, &config_with_notion()).await;
        assert!(matches!(result, Err(ConnectorError::MissingParam(_))));
    }

    #[tokio::test]
    async fn teams_invalid_action() {
        let connector = teams::TeamsConnector;
        let result = connector
            .execute("nonexistent", &HashMap::new(), &config_with_teams())
            .await;
        assert!(matches!(result, Err(ConnectorError::InvalidOperation(_))));
    }

    #[tokio::test]
    async fn teams_post_message_missing_team_id() {
        let connector = teams::TeamsConnector;
        let params: HashMap<String, String> = HashMap::new();
        let result = connector.execute("post_message", &params, &config_with_teams()).await;
        assert!(matches!(result, Err(ConnectorError::MissingParam(_))));
    }
}

#[cfg(test)]
mod connector_names_unique {
    use crate::connectors::*;
    use std::collections::HashSet;

    #[test]
    fn no_duplicate_names() {
        let registry = ConnectorRegistry::new();
        let mut names = HashSet::new();
        for c in registry.all() {
            assert!(names.insert(c.name()), "duplicate connector name: {}", c.name());
        }
    }

    #[test]
    fn no_duplicate_operations() {
        let registry = ConnectorRegistry::new();
        let ops = registry.all_operations();
        let mut seen = HashSet::new();
        for op in &ops {
            assert!(seen.insert(op.clone()), "duplicate operation: {op}");
        }
    }
}
