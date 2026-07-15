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
//! by timeout, not consensus.
//!
//! **Split-brain guard (HA.2b, ADR-0041-Nachtrag).** Two mechanisms keep
//! "two active senders of one identity" a transient instead of a steady
//! state:
//!
//! 1. **Startup arbitration:** a `main` listens for one failover timeout
//!    before it starts sending; a foreign heartbeat of its own identity
//!    means someone already serves — it enters the standby watch instead
//!    of doubling the feed.
//! 2. **Runtime demotion:** an active instance keeps watching the group.
//!    A foreign heartbeat of its own identity (another source address) is
//!    a split brain; the deterministic tie-break — the sender with the
//!    **higher** source address yields — makes exactly one of the two
//!    exit (crash-only: the supervisor restarts it, and the restart's
//!    startup arbitration lands it in standby).

use std::net::{Ipv4Addr, SocketAddr};
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

    /// How long the heartbeat has been silent — the observability surface
    /// (`firefly_main_heartbeat_age_seconds`, HA.2b).
    pub fn age(&self, now: Instant) -> Duration {
        now.duration_since(self.last_seen)
    }

    /// Time until promotion would be due (zero when overdue) — the wait
    /// budget for the next receive.
    pub fn remaining(&self, now: Instant) -> Duration {
        (self.timeout + Duration::from_millis(1)).saturating_sub(now.duration_since(self.last_seen))
    }
}

/// Is `datagram` (received from `src`) a **foreign** heartbeat of our own
/// identity — i.e. split-brain evidence? Our own looped-back heartbeats
/// (src == `own`) never count; neither do foreign SDPS identities or
/// non-CAT065 traffic. Pure and deterministic (HA.2b).
pub fn is_foreign_heartbeat(
    datagram: &[u8],
    expected: DataSourceId,
    own: SocketAddr,
    src: SocketAddr,
) -> bool {
    if src == own {
        return false;
    }
    let Ok(reports) = decode_status_block(datagram) else {
        return false;
    };
    reports
        .iter()
        .any(|r| r.source == expected && r.message_type == MESSAGE_TYPE_SDPS_STATUS)
}

/// The deterministic split-brain tie-break (HA.2b): of two active senders
/// with the same identity, the one with the **higher** source address
/// (ip, port ordering) yields. Both sides see both addresses, so exactly
/// one of the two demotes — no configuration, no coordinator.
pub fn demotion_required(own: SocketAddr, foreign: SocketAddr) -> bool {
    own > foreign
}

/// Startup arbitration (HA.2b): listen on the group for up to `window`
/// and return the source address of the first heartbeat carrying our
/// identity — someone is already serving it — or `None` after a silent
/// window. Called **before** this instance sends anything, so every
/// matching heartbeat is foreign by construction.
pub async fn foreign_heartbeat_within(
    group: Ipv4Addr,
    port: u16,
    expected: DataSourceId,
    window: Duration,
) -> std::io::Result<Option<SocketAddr>> {
    let socket = firefly_multicast::receiver::receiver_socket(group, port).await?;
    let deadline = Instant::now() + window;
    let far = SocketAddr::from(([255, 255, 255, 255], u16::MAX));
    let mut buf = [0u8; 2048];
    loop {
        let now = Instant::now();
        let Some(remaining) = deadline
            .checked_duration_since(now)
            .filter(|d| !d.is_zero())
        else {
            return Ok(None);
        };
        match tokio::time::timeout(remaining, socket.recv_from(&mut buf)).await {
            Ok(Ok((n, src))) => {
                // `own = far` can never equal a real src: every match counts.
                if is_foreign_heartbeat(&buf[..n], expected, far, src) {
                    return Ok(Some(src));
                }
            }
            Ok(Err(e)) => return Err(e),
            Err(_elapsed) => return Ok(None),
        }
    }
}

/// Runtime demotion watch (HA.2b), run by the **active** instance: follow
/// the group and return the foreign sender's address as soon as a
/// split-brain is detected **and** the tie-break says we are the one to
/// yield (the caller then exits, crash-only). While we win the tie-break
/// we hold and keep logging — the other side is expected to yield.
pub async fn run_demotion_watch(
    group: Ipv4Addr,
    port: u16,
    expected: DataSourceId,
    own: SocketAddr,
) -> std::io::Result<SocketAddr> {
    let socket = firefly_multicast::receiver::receiver_socket(group, port).await?;
    info!(%group, port, %own, "demotion watch armed (HA.2b): watching for a second active sender");
    let mut held: u64 = 0;
    let mut buf = [0u8; 2048];
    loop {
        let (n, src) = socket.recv_from(&mut buf).await?;
        if !is_foreign_heartbeat(&buf[..n], expected, own, src) {
            continue;
        }
        if demotion_required(own, src) {
            return Ok(src);
        }
        held += 1;
        if held == 1 || held.is_multiple_of(60) {
            warn!(
                foreign = %src,
                held,
                "split brain: a second active sender carries our identity; holding \
                 (tie-break won) — the other side is expected to yield"
            );
        }
    }
}

/// Run the standby watch: join the multicast group, follow the main's
/// CAT065 heartbeat and return when promotion is due (the heartbeat stayed
/// away longer than `timeout`). Returns `Err` only for socket-level
/// failures — a standby that cannot listen cannot do its job.
/// `on_age` is called with the current heartbeat age on every loop turn
/// (each datagram or expired receive window) — the observability hook
/// behind `firefly_main_heartbeat_age_seconds`.
pub async fn wait_for_promotion(
    group: Ipv4Addr,
    port: u16,
    expected: DataSourceId,
    timeout: Duration,
    on_age: impl Fn(Duration),
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
        on_age(watch.age(now));
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
        wait_for_promotion(group, port, ours, timeout, |_| {})
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

    /// Split-brain classification (HA.2b): our own looped-back heartbeat
    /// never counts as foreign; a matching heartbeat from another address
    /// does; foreign identities and garbage never do. REQ: FR-TRK-050
    #[test]
    fn foreign_heartbeat_classification_is_strict() {
        let ours = DataSourceId::new(25, 2);
        let own: SocketAddr = "192.168.1.10:41000".parse().unwrap();
        let other: SocketAddr = "192.168.1.11:41000".parse().unwrap();
        let hb = Cat065Encoder::new(ours, 1).encode_status(100.0, true);
        let foreign_id = Cat065Encoder::new(DataSourceId::new(99, 9), 1).encode_status(100.0, true);

        assert!(!is_foreign_heartbeat(&hb, ours, own, own), "own loopback");
        assert!(is_foreign_heartbeat(&hb, ours, own, other), "split brain");
        assert!(!is_foreign_heartbeat(&foreign_id, ours, own, other));
        assert!(!is_foreign_heartbeat(b"garbage", ours, own, other));
    }

    /// The tie-break is deterministic and symmetric: of any two distinct
    /// sender addresses exactly ONE side is required to demote — never
    /// both (double outage), never neither (steady split brain).
    /// REQ: FR-TRK-050
    #[test]
    fn tie_break_demotes_exactly_one_side() {
        let pairs = [
            ("10.0.0.1:1000", "10.0.0.2:1000"),
            ("10.0.0.1:1000", "10.0.0.1:1001"),
            ("192.168.9.9:65000", "10.0.0.1:1"),
        ];
        for (a, b) in pairs {
            let a: SocketAddr = a.parse().unwrap();
            let b: SocketAddr = b.parse().unwrap();
            assert_ne!(
                demotion_required(a, b),
                demotion_required(b, a),
                "exactly one of {a} / {b} must yield"
            );
        }
    }

    /// Startup arbitration over real multicast (HA.2b): with an active
    /// sender on the group the window reports its address; on a silent
    /// group it reports `None` after the window. REQ: FR-TRK-050
    #[tokio::test]
    async fn startup_arbitration_detects_an_active_sender() {
        let group = Ipv4Addr::new(239, 255, 0, 62);
        let port = 39_066; // distinct from the other multicast tests
        let ours = DataSourceId::new(25, 2);

        // Silent group: a full window passes, nobody is serving.
        let silent = foreign_heartbeat_within(group, port, ours, Duration::from_millis(150))
            .await
            .expect("watch runs");
        assert!(silent.is_none(), "silent group means nobody serves");

        // Active sender: the window reports it well before it expires.
        let sender = tokio::spawn(async move {
            let socket = firefly_multicast::sender_socket().await.expect("sender");
            let encoder = Cat065Encoder::new(ours, 1);
            for k in 0..20u32 {
                let block = encoder.encode_status(f64::from(k), true);
                let _ = socket.send_to(&block, (group, port)).await;
                tokio::time::sleep(Duration::from_millis(40)).await;
            }
        });
        let found = foreign_heartbeat_within(group, port, ours, Duration::from_secs(3))
            .await
            .expect("watch runs");
        assert!(found.is_some(), "an active sender must be detected");
        sender.await.expect("sender task");
    }

    /// The runtime demotion watch over real multicast (HA.2b): while the
    /// only heartbeats come from our own address it holds; a foreign
    /// heartbeat that wins the tie-break makes it yield promptly.
    /// REQ: FR-TRK-050
    #[tokio::test]
    async fn demotion_watch_ignores_own_and_yields_to_a_winning_foreign() {
        let group = Ipv4Addr::new(239, 255, 0, 62);
        let port = 39_067;
        let ours = DataSourceId::new(25, 2);
        let encoder = Cat065Encoder::new(ours, 1);

        // A steady sender whose datagrams we will first treat as our own.
        let sender_socket = firefly_multicast::sender_socket().await.expect("sender");
        let probe = firefly_multicast::receiver::receiver_socket(group, port)
            .await
            .expect("probe");
        sender_socket
            .send_to(&encoder.encode_status(0.0, true), (group, port))
            .await
            .expect("send");
        let mut buf = [0u8; 2048];
        let (_, own_addr) = probe.recv_from(&mut buf).await.expect("probe recv");
        drop(probe);

        let feed = tokio::spawn({
            let encoder = Cat065Encoder::new(ours, 1);
            async move {
                for k in 1..12u32 {
                    let block = encoder.encode_status(f64::from(k), true);
                    let _ = sender_socket.send_to(&block, (group, port)).await;
                    tokio::time::sleep(Duration::from_millis(40)).await;
                }
            }
        });

        // Own heartbeats only: the watch must hold (we time it out).
        let held = tokio::time::timeout(
            Duration::from_millis(300),
            run_demotion_watch(group, port, ours, own_addr),
        )
        .await;
        assert!(held.is_err(), "own heartbeats must never trigger demotion");

        // Same stream, but now "we" are the maximal address: every foreign
        // heartbeat wins the tie-break against us — we must yield.
        let far: SocketAddr = "255.255.255.255:65535".parse().unwrap();
        let yielded = tokio::time::timeout(
            Duration::from_secs(3),
            run_demotion_watch(group, port, ours, far),
        )
        .await
        .expect("must yield promptly")
        .expect("watch runs");
        assert_eq!(yielded, own_addr, "the foreign winner is reported");
        feed.await.expect("feed task");
    }
}
