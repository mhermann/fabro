use std::collections::HashMap;

use fabro_types::{
    GitCoordinateValidationError, GitRunTarget, RunIntent, RunIntentArgs, RunTarget,
    WorkflowVersionId, normalize_git_commit_sha,
};
use serde_json::json;

fn version_id() -> WorkflowVersionId {
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        .parse()
        .expect("fixture version ID should be valid")
}

fn intent() -> RunIntent {
    RunIntent {
        workflow_version_id: version_id(),
        target:              RunTarget::Git(GitRunTarget {
            repo:         "fabro-sh/fabro".to_string(),
            branch:       "feature/run-intent".to_string(),
            tag:          Some("v1.2.3".to_string()),
            sha:          Some("ABCDEF0123456789ABCDEF0123456789ABCDEF01".to_string()),
            instance_url: None,
        }),
        args:                RunIntentArgs {
            model:            Some("gpt-5.6".to_string()),
            provider:         Some("openai".to_string()),
            inputs:           HashMap::from([
                ("attempts".to_string(), json!(3)),
                ("enabled".to_string(), json!(true)),
            ]),
            labels:           HashMap::from([("team".to_string(), "platform".to_string())]),
            dry_run:          Some(false),
            auto_approve:     Some(true),
            preserve_sandbox: None,
        },
        environment_id:      Some("production".to_string()),
        parent_id:           None,
        title:               Some("Add RunIntent".to_string()),
        goal:                Some("Implement the endpoint".to_string()),
    }
}

#[test]
fn run_intent_args_preserve_tri_state_wire_semantics() {
    let omitted = serde_json::to_value(RunIntentArgs::default()).expect("args should serialize");
    assert_eq!(omitted, json!({}));

    let args = RunIntentArgs {
        dry_run: Some(false),
        auto_approve: Some(true),
        preserve_sandbox: Some(false),
        ..RunIntentArgs::default()
    };
    let value = serde_json::to_value(&args).expect("args should serialize");

    assert_eq!(value["dry_run"], false);
    assert_eq!(value["auto_approve"], true);
    assert_eq!(value["preserve_sandbox"], false);
    assert_eq!(
        serde_json::from_value::<RunIntentArgs>(value).expect("args should deserialize"),
        args
    );
    assert!(
        serde_json::from_value::<RunIntentArgs>(json!({ "unexpected": true })).is_err(),
        "unknown args fields must remain rejected"
    );
}

#[test]
fn run_intent_round_trips_the_strict_git_shape() {
    let intent = intent();
    let value = serde_json::to_value(&intent).expect("intent should serialize");

    assert_eq!(value["target"]["kind"], "git");
    assert_eq!(value["target"]["repo"], "fabro-sh/fabro");
    assert_eq!(value["target"]["branch"], "feature/run-intent");
    assert_eq!(value["target"]["tag"], "v1.2.3");
    assert_eq!(
        serde_json::from_value::<RunIntent>(value).expect("intent should deserialize"),
        intent
    );
}

#[test]
fn run_intent_round_trips_the_strict_none_shape() {
    let mut intent = intent();
    intent.target = RunTarget::None {};

    let value = serde_json::to_value(&intent).expect("intent should serialize");

    assert_eq!(value["target"], json!({ "kind": "none" }));
    assert_eq!(
        serde_json::from_value::<RunIntent>(value).expect("intent should deserialize"),
        intent
    );
}

#[test]
fn run_intent_none_target_rejects_unknown_fields() {
    let mut value = serde_json::to_value(intent()).expect("intent should serialize");
    value["target"] = json!({ "kind": "none", "unexpected": true });

    assert!(serde_json::from_value::<RunIntent>(value).is_err());
}

#[test]
fn run_intent_round_trips_the_strict_folder_shape() {
    let mut intent = intent();
    intent.target = RunTarget::Folder {
        path: "/srv/fabro/workspaces/example".to_string(),
    };

    let value = serde_json::to_value(&intent).expect("intent should serialize");

    assert_eq!(
        value["target"],
        json!({
            "kind": "folder",
            "path": "/srv/fabro/workspaces/example",
        })
    );
    assert_eq!(
        serde_json::from_value::<RunIntent>(value).expect("intent should deserialize"),
        intent
    );
}

#[test]
fn run_intent_folder_target_rejects_unknown_and_missing_fields() {
    let mut value = serde_json::to_value(intent()).expect("intent should serialize");
    value["target"] = json!({
        "kind": "folder",
        "path": "/srv/fabro/workspaces/example",
        "unexpected": true,
    });
    assert!(serde_json::from_value::<RunIntent>(value.clone()).is_err());

    value["target"] = json!({ "kind": "folder" });
    assert!(serde_json::from_value::<RunIntent>(value).is_err());
}

#[test]
fn run_intent_rejects_unknown_fields_at_every_object_boundary() {
    let value = serde_json::to_value(intent()).expect("intent should serialize");

    for path in ["root", "args", "target"] {
        let mut candidate = value.clone();
        match path {
            "root" => candidate["unexpected"] = json!(true),
            "args" => candidate["args"]["unexpected"] = json!(true),
            "target" => candidate["target"]["unexpected"] = json!(true),
            _ => unreachable!(),
        }
        assert!(
            serde_json::from_value::<RunIntent>(candidate).is_err(),
            "{path} should reject unknown fields"
        );
    }
}

#[test]
fn run_intent_requires_args_and_git_branch() {
    let mut missing_args = serde_json::to_value(intent()).expect("intent should serialize");
    missing_args.as_object_mut().unwrap().remove("args");
    assert!(serde_json::from_value::<RunIntent>(missing_args).is_err());

    let mut missing_branch = serde_json::to_value(intent()).expect("intent should serialize");
    missing_branch["target"]
        .as_object_mut()
        .unwrap()
        .remove("branch");
    assert!(serde_json::from_value::<RunIntent>(missing_branch).is_err());
}

#[test]
fn git_commit_sha_normalization_is_exact_and_pure() {
    assert_eq!(
        normalize_git_commit_sha("ABCDEF0123456789ABCDEF0123456789ABCDEF01"),
        Some("abcdef0123456789abcdef0123456789abcdef01".to_string())
    );
    for invalid in [
        "abcdef0123456789abcdef0123456789abcdef0",
        "abcdef0123456789abcdef0123456789abcdef012",
        "abcdef0123456789abcdef0123456789abcdef0g",
        " abcdef0123456789abcdef0123456789abcdef01",
    ] {
        assert_eq!(normalize_git_commit_sha(invalid), None);
    }
}

#[test]
fn target_validation_normalizes_sha_without_network_resolution() {
    let validated = RunTarget::Git(GitRunTarget {
        repo:         "fabro-sh/fabro".to_string(),
        branch:       "feature/run-intent".to_string(),
        tag:          Some("release/v1".to_string()),
        sha:          Some("ABCDEF0123456789ABCDEF0123456789ABCDEF01".to_string()),
        instance_url: None,
    })
    .validate()
    .unwrap();
    let git = validated
        .git
        .expect("Git target should produce a Git projection");

    assert_eq!(
        git.sha.as_deref(),
        Some("abcdef0123456789abcdef0123456789abcdef01")
    );
    assert_eq!(git.origin_url, "https://github.com/fabro-sh/fabro");
    assert_eq!(
        validated.target,
        RunTarget::Git(GitRunTarget {
            repo:         "fabro-sh/fabro".to_string(),
            branch:       "feature/run-intent".to_string(),
            tag:          Some("release/v1".to_string()),
            sha:          Some("abcdef0123456789abcdef0123456789abcdef01".to_string()),
            instance_url: None,
        })
    );
}

#[test]
fn git_target_validation_carries_the_parsed_repository_proof() {
    let validated = GitRunTarget {
        repo:         "Fabro-Sh/Fabro".to_string(),
        branch:       "feature/run-intent".to_string(),
        tag:          None,
        sha:          Some("ABCDEF0123456789ABCDEF0123456789ABCDEF01".to_string()),
        instance_url: None,
    }
    .validate()
    .unwrap();

    assert_eq!(validated.repository().owner(), "Fabro-Sh");
    assert_eq!(validated.repository().repo(), "Fabro");
    assert_eq!(
        validated.target().sha.as_deref(),
        Some("abcdef0123456789abcdef0123456789abcdef01")
    );
}

#[test]
fn run_intent_none_target_validates_without_a_git_projection() {
    let validated = RunTarget::None {}.validate().unwrap();
    assert_eq!(validated.target, RunTarget::None {});
    assert_eq!(validated.git, None);
}

#[test]
fn forge_git_target_validates_with_instance_url() {
    let validated = GitRunTarget {
        repo:         "Fabro-Sh/Fabro".to_string(),
        branch:       "feature/run-intent".to_string(),
        tag:          None,
        sha:          None,
        instance_url: Some("https://forgejo.example.com".to_string()),
    }
    .validate()
    .unwrap();

    assert_eq!(
        validated.instance_url(),
        Some("https://forgejo.example.com")
    );
    assert_eq!(validated.repository().owner(), "Fabro-Sh");
    assert_eq!(validated.repository().repo(), "Fabro");
    assert_eq!(
        validated.target().repo,
        "Fabro-Sh/Fabro",
        "the slug stays unchanged for forge targets"
    );
}

#[test]
fn forge_git_target_builds_instance_origin_url() {
    let validated = RunTarget::Git(GitRunTarget {
        repo:         "fabro-sh/fabro".to_string(),
        branch:       "main".to_string(),
        tag:          None,
        sha:          Some("abcdef0123456789abcdef0123456789abcdef01".to_string()),
        instance_url: Some("http://127.0.0.1:3000".to_string()),
    })
    .validate()
    .unwrap();

    let git = validated
        .git
        .as_ref()
        .expect("forge target has a git projection");
    assert_eq!(
        git.origin_url, "http://127.0.0.1:3000/fabro-sh/fabro",
        "forge targets derive their origin from the instance URL"
    );
}

#[test]
fn forge_git_target_rejects_invalid_instance_url() {
    for bad in [
        "forgejo.example.com",
        "https://",
        "https://user:pass@forgejo.example.com",
        "https://forgejo.example.com/gitea",
        "http://forgejo.example.com",
    ] {
        let error = GitRunTarget {
            repo:         "fabro-sh/fabro".to_string(),
            branch:       "main".to_string(),
            tag:          None,
            sha:          None,
            instance_url: Some(bad.to_string()),
        }
        .validate()
        .unwrap_err();
        assert_eq!(error, GitCoordinateValidationError::Repository);
    }
}

#[test]
fn github_git_target_serialization_omits_instance_url() {
    let target = GitRunTarget {
        repo:         "fabro-sh/fabro".to_string(),
        branch:       "main".to_string(),
        tag:          None,
        sha:          None,
        instance_url: None,
    };
    let value = serde_json::to_value(RunTarget::Git(target.clone())).unwrap();
    assert!(
        serde_json::to_string(&value)
            .unwrap()
            .contains("https://github.com/fabro-sh/fabro")
            || !serde_json::to_string(&value)
                .unwrap()
                .contains("instance_url"),
        "github targets must not carry instance_url on the wire"
    );
    assert!(
        !serde_json::to_string(&value)
            .unwrap()
            .contains("instance_url")
    );
}

#[test]
fn forge_git_target_round_trips_with_instance_url() {
    let target = RunTarget::Git(GitRunTarget {
        repo:         "acme/widgets".to_string(),
        branch:       "main".to_string(),
        tag:          None,
        sha:          None,
        instance_url: Some("https://forgejo.example.com".to_string()),
    });
    let value = serde_json::to_value(&target).expect("target should serialize");
    assert_eq!(
        serde_json::from_value::<RunTarget>(value).expect("target should deserialize"),
        target
    );
}

#[test]
fn git_target_rejects_instance_url_with_invalid_repo_slug() {
    // The shared slug grammar still applies on forge targets.
    let error = GitRunTarget {
        repo:         "acme".to_string(),
        branch:       "main".to_string(),
        tag:          None,
        sha:          None,
        instance_url: Some("https://forgejo.example.com".to_string()),
    }
    .validate()
    .unwrap_err();
    assert_eq!(error, GitCoordinateValidationError::Repository);
}

#[test]
fn run_intent_folder_target_is_preserved_for_provider_admission() {
    let target = RunTarget::Folder {
        path: "/srv/fabro/workspaces/example".to_string(),
    };

    let validated = target.clone().validate().unwrap();

    assert_eq!(validated.target, target);
    assert_eq!(validated.git, None);
}

#[test]
fn target_validation_rejects_invalid_grammar() {
    use fabro_types::TargetValidationError;

    let validate = |repo: &str, branch: &str, tag: Option<&str>, sha: Option<&str>| {
        RunTarget::Git(GitRunTarget {
            repo:         repo.to_string(),
            branch:       branch.to_string(),
            tag:          tag.map(str::to_string),
            sha:          sha.map(str::to_string),
            instance_url: None,
        })
        .validate()
    };

    assert_eq!(
        validate("not-a-slug", "main", None, None).unwrap_err(),
        TargetValidationError::Repository
    );
    for branch in [
        "",
        "HEAD",
        "-main",
        ".main",
        "heads/main",
        "tags/v1",
        "refs/heads/main",
        "abcdef0123456789abcdef0123456789abcdef01",
        "bad..branch",
    ] {
        assert_eq!(
            validate("fabro-sh/fabro", branch, None, None).unwrap_err(),
            TargetValidationError::Branch,
            "{branch:?}"
        );
    }
    for tag in [
        "",
        "HEAD",
        "-v1",
        ".v1",
        "tags/v1",
        "refs/tags/v1",
        "abcdef0123456789abcdef0123456789abcdef01",
        "bad..tag",
    ] {
        assert_eq!(
            validate("fabro-sh/fabro", "main", Some(tag), None).unwrap_err(),
            TargetValidationError::Tag,
            "{tag:?}"
        );
    }
    assert_eq!(
        validate("fabro-sh/fabro", "main", None, Some("short")).unwrap_err(),
        TargetValidationError::Sha
    );
}

#[test]
fn git_target_round_trips_all_branch_tag_sha_states() {
    for target in [
        GitRunTarget {
            repo:         "fabro-sh/fabro".to_string(),
            branch:       "main".to_string(),
            tag:          None,
            sha:          None,
            instance_url: None,
        },
        GitRunTarget {
            repo:         "fabro-sh/fabro".to_string(),
            branch:       "main".to_string(),
            tag:          None,
            sha:          Some("abcdef0123456789abcdef0123456789abcdef01".to_string()),
            instance_url: None,
        },
        GitRunTarget {
            repo:         "fabro-sh/fabro".to_string(),
            branch:       "release".to_string(),
            tag:          Some("v1.2.3".to_string()),
            sha:          None,
            instance_url: None,
        },
        GitRunTarget {
            repo:         "fabro-sh/fabro".to_string(),
            branch:       "release".to_string(),
            tag:          Some("v1.2.3".to_string()),
            sha:          Some("abcdef0123456789abcdef0123456789abcdef01".to_string()),
            instance_url: None,
        },
    ] {
        let run_target = RunTarget::Git(target);
        let value = serde_json::to_value(&run_target).expect("target should serialize");
        assert_eq!(
            serde_json::from_value::<RunTarget>(value).expect("target should deserialize"),
            run_target
        );
    }
}
