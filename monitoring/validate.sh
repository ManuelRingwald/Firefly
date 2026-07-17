#!/usr/bin/env bash
# Validate the Firefly monitoring package (MON.1) — same philosophy as
# deploy/validate.sh: syntax everywhere, tool-backed checks where the tool
# exists, and an explicit SKIP (never silent) where it does not.
set -euo pipefail
cd "$(dirname "$0")"

fail=0

echo "== YAML syntax (alerts + PrometheusRule)"
python3 - <<'EOF' || fail=1
import sys, yaml
for path in ("prometheus/alerts.yaml", "kubernetes/prometheusrule.yaml"):
    try:
        yaml.safe_load(open(path))
        print(f"  ok: {path}")
    except Exception as e:
        print(f"  FAIL: {path}: {e}"); sys.exit(1)
EOF

echo "== JSON syntax (Grafana dashboard)"
python3 - <<'EOF' || fail=1
import json, sys
try:
    d = json.load(open("grafana/dashboard.json"))
    panels = [p for p in d.get("panels", []) if p.get("type") != "row"]
    print(f"  ok: grafana/dashboard.json ({len(panels)} panels)")
except Exception as e:
    print(f"  FAIL: grafana/dashboard.json: {e}"); sys.exit(1)
EOF

echo "== Consistency: PrometheusRule wraps EXACTLY the canonical rules"
python3 - <<'EOF' || fail=1
import sys, yaml
src = yaml.safe_load(open("prometheus/alerts.yaml"))["groups"]
crd = yaml.safe_load(open("kubernetes/prometheusrule.yaml"))["spec"]["groups"]
if src != crd:
    print("  FAIL: kubernetes/prometheusrule.yaml drifted from prometheus/alerts.yaml")
    print("        regenerate it (see README.md) — alerts.yaml is canonical")
    sys.exit(1)
print(f"  ok: {sum(len(g['rules']) for g in src)} rules identical in both files")
EOF

echo "== Every alert has severity + runbook annotation"
python3 - <<'EOF' || fail=1
import sys, yaml
bad = []
for g in yaml.safe_load(open("prometheus/alerts.yaml"))["groups"]:
    for r in g["rules"]:
        if r.get("labels", {}).get("severity") not in ("critical", "warning", "info"):
            bad.append(f"{r['alert']}: missing/unknown severity")
        if "runbook" not in r.get("annotations", {}):
            bad.append(f"{r['alert']}: missing runbook annotation")
for b in bad:
    print(f"  FAIL: {b}")
sys.exit(1 if bad else 0)
EOF
echo "  ok"

echo "== promtool (rule semantics) — where available"
if command -v promtool >/dev/null 2>&1; then
    promtool check rules prometheus/alerts.yaml || fail=1
else
    echo "  SKIP: promtool not installed here — run in CI/operator environment:"
    echo "        promtool check rules monitoring/prometheus/alerts.yaml"
fi

if [ "$fail" -ne 0 ]; then
    echo "VALIDATION FAILED"; exit 1
fi
echo "ALL CHECKS PASSED (see SKIPs above for tool-gated steps)"
