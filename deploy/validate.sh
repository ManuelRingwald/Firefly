#!/usr/bin/env bash
# Validate the Firefly deployment artifacts (HA.3).
#
# Runs everywhere: YAML syntax of the static manifests (python3 + pyyaml).
# Runs where helm exists: `helm lint` and a full `helm template` render of
# the chart (the CI-grade check the sandboxed dev environment cannot run —
# see docs/milestones/HA3-Kubernetes-Deployment.md, honest limits).
set -euo pipefail
cd "$(dirname "$0")"

echo "== YAML syntax (static manifests + chart metadata)"
python3 - <<'EOF'
import yaml
for f, multi in [
    ("kubernetes/firefly.yaml", True),
    ("helm/firefly/Chart.yaml", False),
    ("helm/firefly/values.yaml", False),
]:
    with open(f) as fh:
        docs = list(yaml.safe_load_all(fh)) if multi else [yaml.safe_load(fh)]
    print(f"  {f}: OK ({len(docs)} document(s))")
EOF

if command -v helm >/dev/null 2>&1; then
    echo "== helm lint"
    helm lint helm/firefly
    echo "== helm template (full render, defaults)"
    helm template firefly helm/firefly >/dev/null
    echo "== helm template (standby/snapshot disabled — the degraded shapes render too)"
    helm template firefly helm/firefly \
        --set standby.enabled=false --set snapshot.enabled=false >/dev/null
    echo "helm checks passed"
else
    echo "!! helm not found: chart lint/render SKIPPED (run this where helm exists)"
fi
