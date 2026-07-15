//! Main/Standby roles + heartbeat-watched failover (HA.2a, ADR 0041).
//!
//! One Firefly instance must not be a single point of failure (SDPS-002).
//! A second instance runs as **standby**: it serves its probes but sends
//! nothing (no CAT062/065/063 — one SDPS identity, one sender), and it
//! watches the main's **CAT065 heartbeat** on the multicast group — the
//! signal that exists precisely to distinguish "alive" from "dead"
//! (ADR 0018). When the heartbeat stays away longer than the failover
//! timeout, the standby **promotes** itself: it starts the full live stack,
//! restoring the main's last state snapshot (HA.1) on the way — same track
//! numbers, same identities, same manual pins.
//!
//! Deliberately **no external coordinator** (no etcd, no Kubernetes lease):
//! the wire contract itself carries the liveness signal, observable by any
//! consumer, provider-neutral. The honest limit: this is failure detection
//! by timeout, not consensus — the split-brain guard (a returning main that
//! sees a foreign active heartbeat demotes itself) is HA.2b.

use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

use firefly_asterix::{decode_status_block, DataSourceId, MESSAGE_TYPE_SDPS_STATUS};
use tracing::{debug, info, warn};

/// `FIREFLY_ROLE`: `main` (default) or `standby`.
pub const ROLE_ENV: &str = "FIREFLY_ROLE";
/// `FIREFLY_FAILOVER_TIMEOUT`: seconds without a main heartbeat before the
/// standby promotes itself.
pub const FAILOVER_TIMEOUT_ENV: &str = "FIREFLY_FAILOVER_TIMEOUT";
/// Default failover timeout: three missed heartbeats at the default 1-s
/// CAT065 period — fast enough that the ASD's own staleness banner barely
/// flickers, tolerant enough that one lost datagram does not flap roles.
pub const DEFAULT_FAILOVER_TIMEOUT_S: f64 = 3.0;

/// The instance role (HA.2a).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The active instance: runs sources, tracker and senders (the default).
    Main,
    /// The warm spare: probes only, watches the main's heartbeat, promotes
    /// on its silence.
    Standby,
}

/// Read `FIREFLY_ROLE`. Unset/empty/`main` ⇒ [`Role::Main`]; `standby` ⇒
/// [`Role::Standby`]; anything else is a hard error (a mistyped role must
/// not silently run as a second active sender).
pub fn role_from_env() -> Result<Role, String> {
    parse_role(std::env::var(ROLE_ENV).ok().as_deref())
}

/// The pure parse behind [`role_from_env`] (env vars are process-global,
/// tests run in parallel).
pub fn parse_role(raw: Option<&str>) -> Result<Role, String> {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(Role::Main),
        Some(s) if s.eq_ignore_ascii_case("main") => Ok(Role::Main),
        Some(s) if s.eq_ignore_ascii_case("standby") => Ok(Role::Standby),
        Some(s) => Err(format!(
            "{ROLE_ENV}: expected \"main\" or \"standby\", got {s:?}"
        )),
    }
}

/// Read `FIREFLY_FAILOVER_TIMEOUT` (default 3 s; malformed values are a
/// hard error, meteo pattern).
pub fn failover_timeout_from_env() -> Result<Duration, String> {
    crate::snapshot::parse_positive_secs(
        FAILOVER_TIMEOUT_ENV,
        std::env::var(FAILOVER_TIMEOUT_ENV).ok().as_deref(),
        DEFAULT_FAILOVER_TIMEOUT_S,
    )
    .map(Duration::from_secs_f64)
}

/// The failure detector: tracks when the main's heartbeat was last seen.
///
/// All methods take an explicit `now` so the promotion logic is
/// deterministic and testable without sleeping. The clock starts at
/// construction: if the main is **already dead** when the standby starts,
/// promotion is due one timeout after startup — no heartbeat is ever
/// required to arm the detector.
pub struct HeartbeatWatch {
    last_seen: Instant,
    timeout: Duration,
}

impl HeartbeatWatch {
    pub fn new(now: Instant, timeout: Duration) -> Self {
        Self {
            last_seen: now,
            timeout,
        }
    }

    /// Feed one received datagram. Returns `true` (and re-arms the
    /// detector) if it is a CAT065 SDPS-status heartbeat from `expected`
    /// (our own SAC/SIC — the identity the standby would take over).
    /// Anything else — CAT062/063 traffic, a foreign SDPS, garbage — is
    /// ignored: only the **main's own liveness signal** counts. A NOGO
    /// (degraded) heartbeat still counts as alive: a degraded main is
    /// still the sender, and doubling it would be worse.
    pub fn observe(&mut self, datagram: &[u8], expected: DataSourceId, now: Instant) -> bool {
        let Ok(reports) = decode_status_block(datagram) else {
            return false;
        };
        let is_main = reports
            .iter()
            .any(|r| r.source == expected && r.message_type == MESSAGE_TYPE_SDPS_STATUS);
        if is_main {
            self.last_seen = now;
        }
        is_main
    }

    /// Has the heartbeat been silent for longer than the timeout?
    pub fn promotion_due(&self, now: Instant) -> bool {
        now.duration_since(self.last_seen) > self.timeout
    }

    /// Time until promotion would be due (zero when overdue) — the wait
    /// budget for the next receive.
    pub fn remaining(&self, now: Instant) -> Duration {
        (self.timeout + Duration::from_millis(1)).saturating_sub(now.duration_since(self.last_seen))
    }
}

/// Run the standby watch: join the multicast group, follow the main's
/// CAT065 heartbeat and return when promotion is due (the heartbeat stayed
/// away longer than `timeout`). Returns `Err` only for socket-level
/// failures — a standby that cannot listen cannot do its job.
pub async fn wait_for_promotion(
    group: Ipv4Addr,
    port: u16,
    expected: DataSourceId,
    timeout: Duration,
) -> std::io::Result<()> {
    let socket = firefly_multicast::receiver::receiver_socket(group, port).await?;
    info!(
        %group,
        port,
        sac = expected.sac,
        sic = expected.sic,
        timeout_s = timeout.as_secs_f64(),
        "standby: watching the main's CAT065 heartbeat"
    );
    let mut watch = HeartbeatWatch::new(Instant::now(), timeout);
    let mut heartbeats: u64 = 0;
    let mut buf = [0u8; 2048];
    loop {
        let now = Instant::now();
        if watch.promotion_due(now) {
            warn!(
                heartbeats,
                timeout_s = timeout.as_secs_f64(),
                "standby: main heartbeat silent beyond the failover timeout — promoting"
            );
            return Ok(());
        }
        match tokio::time::timeout(watch.remaining(now), socket.recv_from(&mut buf)).await {
            Ok(Ok((n, _))) => {
                if watch.observe(&buf[..n], expected, Instant::now()) {
                    heartbeats += 1;
                    if heartbeats == 1 {
                        info!("standby: main heartbeat acquired");
                    } else {
                        debug!(heartbeats, "standby: main heartbeat seen");
                    }
                }
            }
            Ok(Err(e)) => return Err(e),
            // Receive window elapsed: the loop head re-checks promotion.
            Err(_elapsed) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use firefly_asterix::Cat065Encoder;

    /// Role parsing: unset/main ⇒ Main, standby ⇒ Standby (case/space
    /// tolerant), anything else is a hard error — a mistyped role must not
    /// silently run as a second active sender. REQ: FR-TRK-050
    #[test]
    fn role_parsing_is_strict() {
        assert_eq!(parse_role(None), Ok(Role::Main));
        assert_eq!(parse_role(Some("")), Ok(Role::Main));
        assert_eq!(parse_role(Some(" main ")), Ok(Role::Main));
        assert_eq!(parse_role(Some("Standby")), Ok(Role::Standby));
        assert!(parse_role(Some("primary")).is_err());
    }

    /// The failure detector: a heartbeat from our SDPS identity re-arms it
    /// (operational or degraded alike), foreign or non-CAT065 datagrams do
    /// not, and silence beyond the timeout makes promotion due — including
    /// the "main already dead at standby start" case where no heartbeat is
    /// ever seen. REQ: FR-TRK-050
    #[test]
    fn heartbeat_watch_promotes_only_on_own_silence() {
        let ours = DataSourceId::new(25, 2);
        let t0 = Instant::now();
        let timeout = Duration::from_secs(3);
        let mut watch = HeartbeatWatch::new(t0, timeout);

        // Fresh start: not yet due, due after the timeout with no heartbeat.
        assert!(!watch.promotion_due(t0 + Duration::from_secs(2)));
        assert!(watch.promotion_due(t0 + Duration::from_secs(4)));

        // A heartbeat from our identity re-arms — degraded (NOGO) too.
        let hb = Cat065Encoder::new(ours, 1).encode_status(100.0, true);
        assert!(watch.observe(&hb, ours, t0 + Duration::from_secs(2)));
        assert!(!watch.promotion_due(t0 + Duration::from_secs(4)));
        let degraded = Cat065Encoder::new(ours, 1).encode_status(101.0, false);
        assert!(watch.observe(&degraded, ours, t0 + Duration::from_secs(4)));
        assert!(!watch.promotion_due(t0 + Duration::from_secs(6)));

        // A foreign SDPS or garbage never re-arms.
        let foreign = Cat065Encoder::new(DataSourceId::new(99, 9), 1).encode_status(102.0, true);
        assert!(!watch.observe(&foreign, ours, t0 + Duration::from_secs(6)));
        assert!(!watch.observe(b"garbage", ours, t0 + Duration::from_secs(6)));
        assert!(watch.promotion_due(t0 + Duration::from_secs(8)));
    }

    /// End-to-end over real multicast loopback: while a fake main sends
    /// heartbeats the standby stays put; once the heartbeats stop it
    /// promotes after (roughly) the failover timeout. REQ: FR-TRK-050
    #[tokio::test]
    async fn standby_promotes_after_the_heartbeat_stops() {
        let group = Ipv4Addr::new(239, 255, 0, 62);
        let port = 39_065; // distinct from other multicast tests
        let ours = DataSourceId::new(25, 2);
        let timeout = Duration::from_millis(200);

        // A fake main: 10 heartbeats, 40 ms apart (~400 ms), then silence.
        let sender = tokio::spawn(async move {
            let socket = firefly_multicast::sender_socket().await.expect("sender");
            let encoder = Cat065Encoder::new(ours, 1);
            for k in 0..10u32 {
                let block = encoder.encode_status(f64::from(k), true);
                let _ = socket.send_to(&block, (group, port)).await;
                tokio::time::sleep(Duration::from_millis(40)).await;
            }
        });

        let started = Instant::now();
        wait_for_promotion(group, port, ours, timeout)
            .await
            .expect("watch runs");
        let elapsed = started.elapsed();
        sender.await.expect("sender task");
        assert!(
            elapsed >= Duration::from_millis(350),
            "must not promote while heartbeats flow (elapsed {elapsed:?})"
        );
        assert!(
            elapsed < Duration::from_secs(5),
            "must promote promptly after silence (elapsed {elapsed:?})"
        );
    }
}
