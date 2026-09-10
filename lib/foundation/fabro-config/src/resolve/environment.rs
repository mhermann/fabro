use std::path::Path;

use fabro_types::settings::run::{
    DockerfileSource, EnvironmentImageSettings, EnvironmentLifecycleSettings,
    EnvironmentNetworkMode, EnvironmentNetworkSettings, EnvironmentProvider,
    EnvironmentResourcesSettings, EnvironmentSettings, RunEnvironmentSettings,
};

use super::ResolveError;
use crate::{
    Combine, EnvironmentDockerfileLayer, EnvironmentImageLayer, EnvironmentLayer,
    EnvironmentLifecycleLayer, EnvironmentNetworkLayer, EnvironmentResourcesLayer, MergeMap,
    RunEnvironmentLayer,
};

pub(crate) fn resolve_run_environment(
    layer: Option<&RunEnvironmentLayer>,
    catalog: &MergeMap<EnvironmentLayer>,
    errors: &mut Vec<ResolveError>,
) -> RunEnvironmentSettings {
    let layer = layer.expect("defaults.toml should provide run.environment defaults");
    let id = layer.id.clone().unwrap_or_else(|| {
        errors.push(ResolveError::Missing {
            path: "run.environment.id".to_string(),
        });
        "default".to_string()
    });

    let Some(base) = catalog.get(&id) else {
        errors.push(ResolveError::Invalid {
            path:   "run.environment.id".to_string(),
            reason: format!("unknown environment: {id}"),
        });
        return RunEnvironmentSettings::from_environment(id, EnvironmentSettings::default());
    };

    let merged = layer
        .clone()
        .into_environment_override()
        .combine(base.clone());
    let environment = resolve_environment_fields(&merged, "run.environment", errors);
    validate_provider_capabilities(&environment, "run.environment", errors);
    RunEnvironmentSettings::from_environment(id, environment)
}

pub fn resolve_environment_layer(
    layer: &EnvironmentLayer,
    path: &str,
) -> Result<EnvironmentSettings, Vec<ResolveError>> {
    let mut errors = Vec::new();
    let environment = resolve_environment_fields(layer, path, &mut errors);
    validate_provider_capabilities(&environment, path, &mut errors);
    if errors.is_empty() {
        Ok(environment)
    } else {
        Err(errors)
    }
}

fn resolve_environment_fields(
    layer: &EnvironmentLayer,
    path: &str,
    errors: &mut Vec<ResolveError>,
) -> EnvironmentSettings {
    let provider = if let Some(raw) = layer.provider.as_deref() {
        parse_provider(raw, &format!("{path}.provider"), errors)
    } else {
        errors.push(ResolveError::Missing {
            path: format!("{path}.provider"),
        });
        EnvironmentProvider::Local
    };

    let environment = EnvironmentSettings {
        provider,
        cwd: resolve_cwd(layer.cwd.as_deref(), &format!("{path}.cwd"), errors),
        image: resolve_image(layer.image.as_ref()),
        resources: resolve_resources(layer.resources.as_ref()),
        network: resolve_network(layer.network.as_ref(), &format!("{path}.network"), errors),
        lifecycle: resolve_lifecycle(layer.lifecycle.as_ref()),
        labels: layer.labels.clone().into_inner(),
        env: layer.env.clone().into_inner(),
    };
    environment
}

fn resolve_cwd(raw: Option<&str>, path: &str, errors: &mut Vec<ResolveError>) -> Option<String> {
    let raw = raw?;
    if raw.trim().is_empty() {
        errors.push(ResolveError::Invalid {
            path:   path.to_string(),
            reason: "cwd must not be empty".to_string(),
        });
        return None;
    }
    if !Path::new(raw).is_absolute() {
        errors.push(ResolveError::Invalid {
            path:   path.to_string(),
            reason: "cwd must be an absolute path".to_string(),
        });
        return None;
    }
    Some(raw.to_string())
}

fn parse_provider(raw: &str, path: &str, errors: &mut Vec<ResolveError>) -> EnvironmentProvider {
    if let Ok(provider) = raw.parse::<EnvironmentProvider>() {
        provider
    } else {
        errors.push(ResolveError::Invalid {
            path:   path.to_string(),
            reason: format!("unknown environment provider: {raw}"),
        });
        EnvironmentProvider::Local
    }
}

fn resolve_image(layer: Option<&EnvironmentImageLayer>) -> EnvironmentImageSettings {
    let Some(layer) = layer else {
        return EnvironmentImageSettings::default();
    };
    EnvironmentImageSettings {
        docker:     layer.docker.clone(),
        dockerfile: layer.dockerfile.as_ref().map(dockerfile_source),
    }
}

fn resolve_resources(layer: Option<&EnvironmentResourcesLayer>) -> EnvironmentResourcesSettings {
    let Some(layer) = layer else {
        return EnvironmentResourcesSettings::default();
    };
    EnvironmentResourcesSettings {
        cpu:    layer.cpu,
        memory: layer.memory,
        disk:   layer.disk,
    }
}

fn resolve_network(
    layer: Option<&EnvironmentNetworkLayer>,
    path: &str,
    errors: &mut Vec<ResolveError>,
) -> EnvironmentNetworkSettings {
    let Some(layer) = layer else {
        return EnvironmentNetworkSettings::default();
    };

    for (index, cidr) in layer.allow.iter().enumerate() {
        if cidr.parse::<ipnet::IpNet>().is_err() {
            errors.push(ResolveError::Invalid {
                path:   format!("{path}.allow[{index}]"),
                reason: format!("invalid CIDR: {cidr}"),
            });
        }
    }

    let mode = match layer.mode.as_deref() {
        Some(raw) => parse_network_mode(raw, &format!("{path}.mode"), errors),
        None if layer.allow.is_empty() => EnvironmentNetworkMode::AllowAll,
        None => EnvironmentNetworkMode::CidrAllowList,
    };

    EnvironmentNetworkSettings {
        mode,
        allow: layer.allow.clone(),
    }
}

fn parse_network_mode(
    raw: &str,
    path: &str,
    errors: &mut Vec<ResolveError>,
) -> EnvironmentNetworkMode {
    if let Ok(mode) = raw.parse::<EnvironmentNetworkMode>() {
        mode
    } else {
        errors.push(ResolveError::Invalid {
            path:   path.to_string(),
            reason: format!("unknown environment network mode: {raw}"),
        });
        EnvironmentNetworkMode::AllowAll
    }
}

fn resolve_lifecycle(layer: Option<&EnvironmentLifecycleLayer>) -> EnvironmentLifecycleSettings {
    let Some(layer) = layer else {
        return EnvironmentLifecycleSettings::default();
    };
    EnvironmentLifecycleSettings {
        preserve:         layer.preserve.unwrap_or(false),
        stop_on_terminal: layer.stop_on_terminal.unwrap_or(true),
        auto_stop:        layer.auto_stop,
    }
}

fn dockerfile_source(dockerfile: &EnvironmentDockerfileLayer) -> DockerfileSource {
    match dockerfile {
        EnvironmentDockerfileLayer::Inline(text) => DockerfileSource::Inline(text.clone()),
        EnvironmentDockerfileLayer::Path { path } => DockerfileSource::Path { path: path.clone() },
    }
}

fn validate_provider_capabilities(
    environment: &EnvironmentSettings,
    path: &str,
    errors: &mut Vec<ResolveError>,
) {
    match environment.provider {
        EnvironmentProvider::Local => {
            if matches!(
                environment.network.mode,
                EnvironmentNetworkMode::Block | EnvironmentNetworkMode::CidrAllowList
            ) {
                errors.push(ResolveError::Invalid {
                    path:   format!("{path}.network.mode"),
                    reason:
                        "local environments cannot enforce blocked or CIDR allow-list networking"
                            .to_string(),
                });
            }
        }
        EnvironmentProvider::Docker => {
            if environment.network.mode == EnvironmentNetworkMode::CidrAllowList {
                errors.push(ResolveError::Invalid {
                    path:   format!("{path}.network.mode"),
                    reason: "docker environments cannot enforce CIDR allow-list networking"
                        .to_string(),
                });
            }
        }
        EnvironmentProvider::Daytona => {
            if environment.image.docker.is_some() && environment.image.dockerfile.is_some() {
                errors.push(ResolveError::Invalid {
                    path:   format!("{path}.image"),
                    reason: "daytona environments accept either image.docker or image.dockerfile, \
                             not both"
                        .to_string(),
                });
            }
        }
        EnvironmentProvider::Kubernetes => {
            if environment.image.dockerfile.is_some() {
                errors.push(ResolveError::Invalid {
                    path:   format!("{path}.image.dockerfile"),
                    reason: "kubernetes environments cannot build images; set image.docker to a \
                             prebuilt image reference and publish it to a registry the cluster \
                             can pull from"
                        .to_string(),
                });
            }
            if matches!(
                environment.network.mode,
                EnvironmentNetworkMode::Block | EnvironmentNetworkMode::CidrAllowList
            ) {
                errors.push(ResolveError::Invalid {
                    path:   format!("{path}.network.mode"),
                    reason: "kubernetes environments cannot enforce blocked or CIDR allow-list \
                         networking"
                        .to_string(),
                });
            }
            for (key, value) in &environment.labels {
                if let Some(reason) = kubernetes_label_problem(key, true) {
                    errors.push(ResolveError::Invalid {
                        path: format!("{path}.labels.{key}"),
                        reason,
                    });
                }
                if let Some(reason) = kubernetes_label_problem(value, false) {
                    errors.push(ResolveError::Invalid {
                        path:   format!("{path}.labels.{key}"),
                        reason: format!("invalid label value: {reason}"),
                    });
                }
            }
        }
    }
}

/// Validate a Kubernetes label key or value against the restricted subset
/// Fabro maps onto pod labels. Keys may carry a `/`-separated prefix, which
/// must be a DNS subdomain; the name half and any value must be 63 characters
/// or fewer.
fn kubernetes_label_problem(value: &str, is_key: bool) -> Option<String> {
    let (prefix, segment) = if is_key {
        value
            .split_once('/')
            .map_or((None, value), |(p, n)| (Some(p), n))
    } else {
        (None, value)
    };
    if segment.is_empty() {
        return Some("must not be empty".to_string());
    }
    if segment.len() > 63 {
        return Some("must be at most 63 characters".to_string());
    }
    let valid_char = |byte: u8| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.');
    if !segment.bytes().all(valid_char) {
        return Some("must contain only ASCII letters, digits, '-', '_', and '.'".to_string());
    }
    if !segment.starts_with(|byte: char| byte.is_ascii_alphanumeric())
        || !segment.ends_with(|byte: char| byte.is_ascii_alphanumeric())
    {
        return Some("must start and end with an ASCII letter or digit".to_string());
    }
    if let Some(prefix) = prefix {
        let valid_prefix = |byte: u8| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'.')
        };
        if prefix.is_empty() || prefix.len() > 253 || !prefix.bytes().all(valid_prefix) {
            return Some(
                "label key prefix must be a lowercase DNS subdomain of at most 253 characters"
                    .to_string(),
            );
        }
    }
    None
}
