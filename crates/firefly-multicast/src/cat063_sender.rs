//! The CAT063 **sensor status** sender — the per-sensor health counterpart to
//! the CAT065 SDPS-liveness heartbeat ([`crate::heartbeat`]).
//!
//! One CAT063 block per `period`, carrying one record per registered sensor.
//! Each record reports whether that sensor is currently active (receiving
//! plots) or degraded (silent for more than 2.5 × its scan period).
//!
//! Consumers (Wayfinder) receive these blocks on the **same** multicast
//! group/port as CAT062 and CAT065 and dispatch on the leading CAT byte
//! (`0x3F` = 63).  Wayfinder uses the sensor counts to drive the yellow
//! "sensor degradation" state of its feed-health indicator (Firefly #32).

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use firefly_asterix::{Cat063Encoder, SensorReport, SsrBias};
use tokio::net::UdpSocket;

use crate::sensor_health::SensorHealthMonitor;

/// Send one CAT063 Sensor Status block every `period` (wall-clock).
///
/// Each tick queries `monitor` for the current sensor health, encodes one
/// record per sensor and sends the block to `destination`.  The time of day
/// is provided by `now_time_of_day()` (seconds since UTC midnight).
///
/// `applied_biases` is queried once per tick for the registration corrections
/// currently applied by the SDPS (REG.3, ADR 0034), keyed by sensor id; a
/// sensor with an entry carries I063/080 + I063/081 in its record. Callers
/// without registration pass a closure returning an empty map.
///
/// `on_sent` is called after each successful send with `(sensors_active,
/// sensors_total)` — callers use this to update Prometheus gauges.
///
/// The loop never returns on its own; it stops when a send fails, returning
/// the `io::Error` to the caller (a spawned task).
// A task entry point wired up exactly once (main.rs): each argument is an
// independent collaborator (socket, target, codec, three callbacks); bundling
// them into a one-use struct would add ceremony, not clarity.
#[allow(clippy::too_many_arguments)]
pub async fn run_cat063_sender(
    socket: &UdpSocket,
    destination: SocketAddr,
    encoder: &Cat063Encoder,
    monitor: Arc<SensorHealthMonitor>,
    period: Duration,
    mut now_time_of_day: impl FnMut() -> f64,
    mut applied_biases: impl FnMut() -> BTreeMap<u16, SsrBias>,
    mut on_sent: impl FnMut(usize, usize),
) -> std::io::Result<()> {
    let mut ticker = tokio::time::interval(period);
    loop {
        ticker.tick().await;
        let snapshot = monitor.snapshot(Instant::now());
        let biases = applied_biases();
        let sensors: Vec<SensorReport> = snapshot
            .per_sensor
            .iter()
            .map(|(id, health)| SensorReport {
                sic: id.0 as u8,
                operational: health.active,
                reason: health.reason,
                ssr_bias: biases.get(&id.0).copied(),
            })
            .collect();

        let block = encoder.encode(now_time_of_day(), &sensors);
        match socket.send_to(&block, destination).await {
            Ok(bytes) => {
                tracing::debug!(
                    bytes,
                    sensors_total = snapshot.sensors_total,
                    sensors_active = snapshot.sensors_active,
                    %destination,
                    "sent CAT063 sensor status block"
                );
                on_sent(snapshot.sensors_active, snapshot.sensors_total);
            }
            Err(error) => {
                tracing::error!(%destination, %error, "failed to send CAT063 sensor status block");
                return Err(error);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use firefly_asterix::{decode_sensor_block, DataSourceId};
    use firefly_core::SensorId;
    use std::net::Ipv4Addr;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A loopback receiver gets a decodable CAT063 block with the correct
    /// sensor count, and `on_sent` fires once per tick.
    #[tokio::test]
    async fn cat063_blocks_are_sent_and_decodable() {
        let receiver = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let destination = receiver.local_addr().unwrap();
        let sender = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();

        let encoder = Cat063Encoder::new(DataSourceId::new(25, 2), 0);
        let monitor = Arc::new(SensorHealthMonitor::new_preseeded([
            SensorId(1),
            SensorId(2),
        ]));

        let sent_count = Arc::new(AtomicUsize::new(0));
        let sent_in_task = Arc::clone(&sent_count);
        let monitor_task = Arc::clone(&monitor);

        let handle = tokio::spawn(async move {
            let _ = run_cat063_sender(
                &sender,
                destination,
                &encoder,
                monitor_task,
                Duration::from_millis(5),
                || 0.0,
                BTreeMap::new,
                move |active, total| {
                    assert_eq!(total, 2);
                    assert_eq!(active, 2); // replay mode: all active
                    sent_in_task.fetch_add(1, Ordering::Relaxed);
                },
            )
            .await;
        });

        let mut buf = [0u8; 128];
        for _ in 0..2 {
            let (n, _) = receiver.recv_from(&mut buf).await.unwrap();
            let records = decode_sensor_block(&buf[..n]).expect("decodes");
            assert_eq!(records.len(), 2, "one record per sensor");
            for r in &records {
                assert!(r.operational, "replay mode: all sensors operational");
                assert_eq!(r.ssr_bias, None, "no registration → no bias items");
            }
        }
        handle.abort();
        assert!(
            sent_count.load(Ordering::Relaxed) >= 2,
            "on_sent fired per tick"
        );
    }

    /// A sensor with an applied registration correction carries its bias in
    /// the CAT063 block on the wire; sensors without stay plain (REG.3).
    #[tokio::test]
    async fn applied_bias_reaches_the_wire() {
        let receiver = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let destination = receiver.local_addr().unwrap();
        let sender = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();

        let encoder = Cat063Encoder::new(DataSourceId::new(25, 2), 0);
        let monitor = Arc::new(SensorHealthMonitor::new_preseeded([
            SensorId(1),
            SensorId(2),
        ]));

        let handle = tokio::spawn(async move {
            let _ = run_cat063_sender(
                &sender,
                destination,
                &encoder,
                monitor,
                Duration::from_millis(5),
                || 0.0,
                || {
                    BTreeMap::from([(
                        1_u16,
                        SsrBias {
                            range_gain: 0.0,
                            range_bias_m: 150.0,
                            azimuth_bias_deg: 0.3,
                        },
                    )])
                },
                |_, _| {},
            )
            .await;
        });

        let mut buf = [0u8; 128];
        let (n, _) = receiver.recv_from(&mut buf).await.unwrap();
        handle.abort();

        let records = decode_sensor_block(&buf[..n]).expect("decodes");
        assert_eq!(records.len(), 2);
        let with_bias = records
            .iter()
            .find(|r| r.sensor.sic == 1)
            .expect("sensor 1 present");
        let bias = with_bias.ssr_bias.expect("sensor 1 carries its bias");
        assert!((bias.range_bias_m - 150.0).abs() < 15.0, "within one LSB");
        assert!((bias.azimuth_bias_deg - 0.3).abs() < 0.006);
        let plain = records
            .iter()
            .find(|r| r.sensor.sic == 2)
            .expect("sensor 2 present");
        assert_eq!(plain.ssr_bias, None, "no correction → no bias items");
    }
}
