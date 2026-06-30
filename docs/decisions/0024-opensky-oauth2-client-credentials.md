# ADR 0024 — OpenSky-Authentifizierung: OAuth2 Client-Credentials statt Basic Auth

- **Status:** akzeptiert
- **Datum:** 2026-06-29
- **Schnittstellen-relevant:** teilweise. Der `FIREFLY_SOURCES`-**Wire-Vertrag**
  (ADR 0023, `docs/source-input-contract.md`) bricht **nicht** — der Cred-Wert
  bleibt ein String mit einem Doppelpunkt. Aber seine **Bedeutung** ändert sich von
  `benutzer:passwort` zu `client_id:client_secret`; die Vertrags-Doku und Wayfinders
  UI-Beschriftung ziehen nach. Der **Ausgabe**-Vertrag (CAT062/UDP, ICD) ist
  unberührt.
- **Auslöser:** OpenSky hat HTTP Basic Auth abgeschaltet und akzeptiert für die
  REST-API **ausschließlich** den OAuth2-Client-Credentials-Flow. Fireflys
  `firefly-opensky`-Adapter authentifizierte per `basic_auth(user, pass)` — damit
  schlägt **jede** authentifizierte Anfrage jetzt mit `401` fehl.

## Kontext

ADR 0019 führte den OpenSky-ADS-B-Adapter ein, der die REST-API mit optionalem
HTTP-Basic-Auth (`FIREFLY_OPENSKY_USERNAME`/`_PASSWORD`) pollt. OpenSky hat das
Authentifizierungsmodell umgestellt:

> „OpenSky exclusively supports the OAuth2 client credentials flow. Basic
> authentication with username and password is no longer accepted."

Der Betreiber legt auf der OpenSky-Account-Seite einen **API-Client** an und erhält
`client_id` + `client_secret`. Diese werden am Keycloak-Token-Endpoint gegen ein
kurzlebiges (~30 min) **Bearer-Access-Token** getauscht, das an jede API-Anfrage
gehängt wird. Ein abgelaufenes Token liefert `401` → neues Token holen und erneut
versuchen.

Ohne diese Migration ist authentifiziertes Live-ADS-B kaputt — und damit die
authentifizierte End-to-End-Abnahme der Wayfinder-Auto-Orchestrierung (ADR 0012
dort) nicht durchführbar.

## Entscheidung

### 1. Auth-Mechanismus: OAuth2 Client-Credentials

Der Adapter holt vor dem Poll ein Access-Token am Token-Endpoint
(`grant_type=client_credentials`, `client_id`/`client_secret` im **form-kodierten
POST-Body**, nie in der URL) und hängt es als `Authorization: Bearer` an die
`states/all`-Anfrage. Anonymes (unauthentifiziertes) Pollen bleibt erhalten —
ohne Credentials kein Token, kein Header (engeres Rate-Limit).

### 2. Token-Lebenszyklus: Cache + proaktiver Refresh + 401-Recovery

Ein `TokenCache` hält Token und Ablauf:

- **Reuse** eines gecachten Tokens, solange es nicht innerhalb einer **Skew**
  (60 s) vor Ablauf liegt — so trägt keine Anfrage ein fast-abgelaufenes Token.
- **Proaktiver Refresh**, sobald die Skew-Schwelle erreicht ist.
- **Reaktive Recovery:** Ein `401` trotz (scheinbar) gültigem Cache (serverseitiger
  Ablauf, Widerruf) invalidiert den Cache und versucht die Anfrage **genau einmal**
  erneut mit frischem Token.
- Der Lock wird über den Token-Fetch gehalten → konkurrierende Poller koalieren auf
  **eine** Token-Anfrage statt den Endpoint zu stürmen.

Die Reuse/Refresh-Entscheidung ist eine **reine** Funktion (`needs_refresh`) und der
Fetch-Schritt ist als Closure injiziert → der Zustandsautomat ist **ohne Netz und
ohne echte Uhr** unit-testbar.

### 3. Konfiguration

`FIREFLY_OPENSKY_USERNAME`/`_PASSWORD` → **`FIREFLY_OPENSKY_CLIENT_ID`** /
**`_CLIENT_SECRET`**. Neu: **`FIREFLY_OPENSKY_TOKEN_URL`** (Default OpenSkys
Keycloak-Realm; überschreibbar für Tests/Realm-Wechsel). Im orchestrierten Pfad
(`FIREFLY_SOURCES`) liefert Wayfinder den Cred-Wert weiterhin als **einen** String;
der Adapter splittet am **ersten** `:` zu `client_id` + `client_secret` (OAuth2
Client-IDs enthalten kein `:`) — der Split-Code ist identisch zu vorher, nur die
Feld-Semantik ändert sich.

## Konsequenzen

**Positiv:**
- Authentifiziertes OpenSky funktioniert wieder; höheres Rate-Limit (~5 s).
- Token-Handling ist isoliert (`auth.rs`), testbar und secret-arm (Secrets nur im
  POST-Body, nie in URL/Log).

**Kosten / Schnittstellen-Wirkung:**
- **Env-Umbenennung** (Breaking für Standalone-Betreiber): Wer
  `FIREFLY_OPENSKY_USERNAME/PASSWORD` setzte, muss auf `CLIENT_ID/CLIENT_SECRET`
  umstellen und auf der OpenSky-Seite einen API-Client anlegen.
- **Wayfinder-Seite:** Der Wire-Vertrag bleibt; die UI-Labels „Benutzername/
  Passwort" werden zu „Client-ID/Client-Secret" (separater Wayfinder-PR). Cross-
  Project notiert in `docs/cross-project/todo-for-wayfinder.md`.

**Ehrliche Grenze:** Token-Refresh und 401-Recovery sind unit-getestet
(Cache-Zustandsautomat, reine Refresh-Entscheidung); der **echte** HTTP-Tausch
gegen OpenSkys Keycloak ist nicht in CI nachgestellt (kein Netz/Account in CI) und
wird in der End-to-End-Abnahme verifiziert.
