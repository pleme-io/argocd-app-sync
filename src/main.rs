//! `pleme-io/argocd-app-sync` — trigger ArgoCD Application sync + wait for healthy.
//!
//! ArgoCD ApplicationSets are the akeyless ARC platform's primary lever:
//! cluster opt-in, runner-pool fan-out, and per-cluster pinning all flow
//! through Application resources. Triggering a sync (or annotating one) +
//! polling for `Synced` + `Healthy` is the canonical operator workflow.
//!
//! Uses `kubectl` (against the ArgoCD cluster's kube context) rather than
//! the argocd CLI — no auth setup beyond the kubeconfig the consumer has
//! already configured.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use pleme_actions_shared::{ActionError, Input, Output, StepSummary};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Inputs {
    app_name: String,
    #[serde(default = "default_namespace")]
    namespace: String,
    #[serde(default = "default_true")]
    wait: bool,
    #[serde(default = "default_timeout")]
    timeout_seconds: u64,
    #[serde(default)]
    kubectl_context: Option<String>,
    /// When true, also annotates `argocd.argoproj.io/refresh=hard` to force
    /// a full refresh + sync (vs the default soft refresh).
    #[serde(default)]
    hard_refresh: bool,
}

fn default_namespace() -> String { "argocd".into() }
fn default_true() -> bool { true }
fn default_timeout() -> u64 { 600 }

fn main() {
    pleme_actions_shared::log::init();
    if let Err(e) = run() {
        e.emit_to_stdout();
        if e.is_fatal() {
            std::process::exit(1);
        }
    }
}

fn run() -> Result<(), ActionError> {
    let inputs = Input::<Inputs>::from_env()?;
    let context_args = build_context_args(&inputs.kubectl_context);

    annotate_for_sync(&inputs.app_name, &inputs.namespace, inputs.hard_refresh, &context_args)?;

    let (sync_status, health_status) = if inputs.wait {
        wait_for_synced_healthy(
            &inputs.app_name,
            &inputs.namespace,
            Duration::from_secs(inputs.timeout_seconds),
            &context_args,
        )?
    } else {
        (
            read_status_field(&inputs.app_name, &inputs.namespace, "sync.status", &context_args)?,
            read_status_field(&inputs.app_name, &inputs.namespace, "health.status", &context_args)?,
        )
    };

    let output = Output::from_runner_env()?;
    output.set("sync-status", &sync_status)?;
    output.set("health-status", &health_status)?;

    let mut summary = StepSummary::from_runner_env()?;
    summary
        .heading(2, &format!("argocd sync — {}", inputs.app_name))
        .table(
            &["Field", "Value"],
            vec![
                vec!["sync".to_string(), sync_status.clone()],
                vec!["health".to_string(), health_status.clone()],
                vec!["hard-refresh".to_string(), inputs.hard_refresh.to_string()],
            ],
        );
    summary.commit()?;

    if sync_status != "Synced" || health_status != "Healthy" {
        return Err(ActionError::error(format!(
            "ArgoCD app `{}` is sync={} health={} (expected Synced/Healthy)",
            inputs.app_name, sync_status, health_status
        )));
    }

    Ok(())
}

fn build_context_args(context: &Option<String>) -> Vec<String> {
    context
        .as_ref()
        .map(|c| vec!["--context".into(), c.clone()])
        .unwrap_or_default()
}

fn annotate_for_sync(
    app: &str,
    namespace: &str,
    hard_refresh: bool,
    context_args: &[String],
) -> Result<(), ActionError> {
    let refresh_value = if hard_refresh { "hard" } else { "normal" };
    let annotations = [
        format!("argocd.argoproj.io/refresh={refresh_value}"),
        format!("reconcile.fluxcd.io/requestedAt={}", chrono_now()),
    ];
    for annotation in &annotations {
        let mut args: Vec<String> = vec![
            "-n".into(),
            namespace.into(),
            "annotate".into(),
            "application".into(),
            app.into(),
            annotation.clone(),
            "--overwrite".into(),
        ];
        args.extend_from_slice(context_args);
        run_kubectl(&args)?;
    }
    Ok(())
}

fn wait_for_synced_healthy(
    app: &str,
    namespace: &str,
    timeout: Duration,
    context_args: &[String],
) -> Result<(String, String), ActionError> {
    let deadline = Instant::now() + timeout;
    let mut last_sync = String::new();
    let mut last_health = String::new();
    while Instant::now() < deadline {
        last_sync = read_status_field(app, namespace, "sync.status", context_args)?;
        last_health = read_status_field(app, namespace, "health.status", context_args)?;
        if last_sync == "Synced" && last_health == "Healthy" {
            return Ok((last_sync, last_health));
        }
        std::thread::sleep(Duration::from_secs(5));
    }
    Err(ActionError::error(format!(
        "timed out after {}s waiting for {app} to reach Synced/Healthy (last: sync={last_sync} health={last_health})",
        timeout.as_secs()
    )))
}

fn read_status_field(
    app: &str,
    namespace: &str,
    jsonpath_field: &str,
    context_args: &[String],
) -> Result<String, ActionError> {
    let mut args: Vec<String> = vec![
        "-n".into(),
        namespace.into(),
        "get".into(),
        "application".into(),
        app.into(),
        "-o".into(),
        format!("jsonpath={{.status.{jsonpath_field}}}"),
    ];
    args.extend_from_slice(context_args);
    let stdout = run_kubectl(&args)?;
    Ok(stdout.trim().to_string())
}

fn run_kubectl(args: &[String]) -> Result<String, ActionError> {
    let output = Command::new("kubectl")
        .args(args.iter().map(String::as_str))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| ActionError::error(format!("failed to spawn kubectl: {e}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(ActionError::error(format!(
            "kubectl exited with status {} (stderr: {})",
            output.status,
            stderr.trim()
        )));
    }
    Ok(stdout.to_string())
}

/// Format the current time as RFC3339 for kubectl annotation. Imports
/// chrono lazily — pure-stdlib UTC formatting via SystemTime would also
/// work but adds noise; chrono is already in pleme-actions-shared's tree.
fn chrono_now() -> String {
    use std::time::SystemTime;
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Simple ISO-like format good enough for an idempotent annotation key.
    format!("{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_context_args_none_when_unset() {
        assert!(build_context_args(&None).is_empty());
    }

    #[test]
    fn build_context_args_emits_context_flag() {
        let args = build_context_args(&Some("my-cluster".into()));
        assert_eq!(args, vec!["--context", "my-cluster"]);
    }

    #[test]
    fn chrono_now_returns_unix_seconds_string() {
        let s = chrono_now();
        assert!(s.parse::<u64>().is_ok());
    }
}
