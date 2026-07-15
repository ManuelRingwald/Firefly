{{/*
Common labels (Kubernetes recommended set).
*/}}
{{- define "firefly.labels" -}}
app.kubernetes.io/name: firefly
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{/*
Selector labels shared by BOTH deployments: the service selects on these,
and readiness does the routing — the standby answers /ready with 503
("standby"), so only the active instance receives traffic (HA.2a).
*/}}
{{- define "firefly.selectorLabels" -}}
app.kubernetes.io/name: firefly
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
The shared pod template body for one role. Scope: dict with "root" (the
chart root context) and "role" ("main" | "standby").
*/}}
{{- define "firefly.podSpec" -}}
{{- $root := .root -}}
{{- $role := .role -}}
{{- with $root.Values.terminationGracePeriodSeconds }}
terminationGracePeriodSeconds: {{ . }}
{{- end }}
{{- if $root.Values.hostNetwork }}
hostNetwork: true
dnsPolicy: ClusterFirstWithHostNet
# Same host port + same multicast socket: the pair must not share a node.
affinity:
  podAntiAffinity:
    requiredDuringSchedulingIgnoredDuringExecution:
      - topologyKey: kubernetes.io/hostname
        labelSelector:
          matchLabels:
            {{- include "firefly.selectorLabels" $root | nindent 12 }}
{{- end }}
securityContext:
  {{- toYaml $root.Values.podSecurityContext | nindent 2 }}
containers:
  - name: firefly
    image: "{{ $root.Values.image.repository }}:{{ $root.Values.image.tag }}"
    imagePullPolicy: {{ $root.Values.image.pullPolicy }}
    securityContext:
      {{- toYaml $root.Values.containerSecurityContext | nindent 6 }}
    env:
      - name: FIREFLY_ROLE
        value: {{ $role | quote }}
    envFrom:
      - configMapRef:
          name: {{ $root.Release.Name }}-firefly-env
      {{- if $root.Values.existingSecret }}
      - secretRef:
          name: {{ $root.Values.existingSecret }}
      {{- else if $root.Values.secrets }}
      - secretRef:
          name: {{ $root.Release.Name }}-firefly-secrets
      {{- end }}
    ports:
      - name: http
        containerPort: 8080
    # Liveness = the process serves; readiness = it may receive traffic.
    # The standby is alive but deliberately not ready (503 "standby").
    livenessProbe:
      httpGet:
        path: /health
        port: http
      periodSeconds: 10
      failureThreshold: 3
    readinessProbe:
      httpGet:
        path: /ready
        port: http
      periodSeconds: 5
      failureThreshold: 2
    resources:
      {{- toYaml $root.Values.resources | nindent 6 }}
    {{- if $root.Values.snapshot.enabled }}
    volumeMounts:
      - name: snapshot
        mountPath: /var/lib/firefly
    {{- end }}
{{- if $root.Values.snapshot.enabled }}
volumes:
  - name: snapshot
    persistentVolumeClaim:
      claimName: {{ $root.Release.Name }}-firefly-snapshot
{{- end }}
{{- end }}
