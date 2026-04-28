# pleme-io/argocd-app-sync

Trigger an ArgoCD Application sync via kubectl annotation; wait for `Synced` + `Healthy`.

## Usage

```yaml
- uses: pleme-io/argocd-app-sync@v1
  with:
    app-name: my-arc-controller
    namespace: argocd
    timeout-seconds: 600
```

## Inputs

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `app-name` | string | yes | — | ArgoCD Application name |
| `namespace` | string | no | `argocd` | ArgoCD namespace |
| `wait` | bool | no | `true` | Wait for Synced + Healthy |
| `timeout-seconds` | number | no | `600` | Wait timeout |
| `kubectl-context` | string | no | — | kubectl context |
| `hard-refresh` | bool | no | `false` | Force `refresh=hard` |

## Outputs

| Name | Type | Description |
|---|---|---|
| `sync-status` | string | Synced / OutOfSync / Unknown |
| `health-status` | string | Healthy / Degraded / Progressing / Missing / Suspended / Unknown |
