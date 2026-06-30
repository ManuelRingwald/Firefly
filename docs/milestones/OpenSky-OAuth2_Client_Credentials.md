# OpenSky-Auth: OAuth2 Client-Credentials (ADR 0024)

> Migration des OpenSky-ADS-B-Adapters von HTTP Basic Auth auf den OAuth2
> Client-Credentials-Flow — OpenSky hat Basic Auth abgeschaltet. Voraussetzung für
> authentifiziertes Live-ADS-B und damit für die End-to-End-Abnahme der
> Wayfinder-Auto-Orchestrierung.

## Fachlich

OpenSky akzeptiert für die REST-API nur noch OAuth2 Client-Credentials. Der
Adapter authentifizierte per `basic_auth(user, pass)` → jede authentifizierte
Anfrage scheitert jetzt mit `401`. Der Betreiber legt auf der OpenSky-Account-Seite
einen API-Client an (`client_id` + `client_secret`); Firefly tauscht sie selbst
gegen ein kurzlebiges Bearer-Token — kein manueller Token-Schritt für den Betreiber.

## Technisch (`crates/firefly-opensky`)

- **`auth.rs` (neu) — Token-Manager:**
  - `TokenCache` hält Token + Ablauf; `token(fetch)` gibt ein gültiges Token zurück
    und holt nur dann neu, wenn keins gecacht ist oder das gecachte innerhalb der
    **Skew** (60 s) vor Ablauf liegt. Der Lock wird über den Fetch gehalten →
    konkurrierende Aufrufe koalieren auf **eine** Token-Anfrage.
  - `invalidate()` für die 401-Recovery.
  - `fetch_token_http` tauscht `client_id`/`client_secret` am Token-Endpoint
    (form-kodierter POST-Body, nie URL) gegen `access_token` + `expires_in`.
  - Reuse/Refresh-Entscheidung als **reine** Funktion `needs_refresh`; der Fetch als
    **injizierte Closure** → Zustandsautomat ohne Netz/Uhr testbar.
- **`config.rs`:** `username`/`password` → `client_id`/`client_secret`
  (`FIREFLY_OPENSKY_CLIENT_ID`/`_CLIENT_SECRET`); neu `token_url`
  (`FIREFLY_OPENSKY_TOKEN_URL`, Default OpenSky-Keycloak).
- **`poller.rs`:** `send_states` hängt bei gesetzten Credentials ein `Bearer`-Token
  an; `fetch_states` macht bei `401` **genau einen** Retry mit invalidiertem Cache.
  Anonym (ohne Credentials) unverändert, kein Retry.
- **`firefly-server/src/sources.rs`:** Cred-Split bleibt am ersten `:`, jetzt in
  `client_id`/`client_secret`; Doc/Fehlermeldungen nachgezogen.

## Sicherheit

- Secrets reisen nur im POST-Body, nie in URL oder Log; `AuthError` druckt kein
  Token. Anonymer Pfad bleibt (engeres Rate-Limit).
- 401-Recovery deckt serverseitigen Ablauf/Widerruf vor der gecachten Ablaufzeit ab.

## Tests

`firefly-opensky` (Unit): `auth::reuses_cached_token_until_due`,
`…refetches_after_invalidate`, `…fetch_error_propagates`,
`…needs_refresh_respects_skew`; `config::token_url_defaults_and_overrides` + die
umbenannten Config-Tests. `firefly-server::sources::cred_env_is_split_into_client_id_and_secret`
+ `…cred_split_uses_the_first_colon`. `cargo test/clippy/fmt --workspace` grün.

## Schnittstellen-Wirkung

`FIREFLY_SOURCES`-Wire-Vertrag **unverändert** (ein String, ein `:`); Bedeutung des
Cred-Werts → `client_id:client_secret` (`source-input-contract.md` v1.1.0).
Wayfinder-Folge: UI-Labels „Benutzername/Passwort" → „Client-ID/Client-Secret"
(separater Wayfinder-PR; notiert in `docs/cross-project/todo-for-wayfinder.md`).

## Ehrliche Grenze

Der echte HTTP-Tausch gegen OpenSkys Keycloak ist nicht in CI nachgestellt (kein
Netz/Account) — er wird in der End-to-End-Abnahme verifiziert. Getestet ist der
Cache-/Refresh-Zustandsautomat und die 401-Retry-Logik.
