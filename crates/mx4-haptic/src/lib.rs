//! Haptic triggers: plays a waveform when the GNOME extension reports a
//! desktop event, or when a desktop notification is sent.
//!
//! ```text
//! GNOME extension ──D-Bus signal──▶ mx4d (this module) ──HID++──▶ mouse
//! any app ──Notify() call──▶ notification server   (watched as a bus monitor)
//! ```

use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use futures_util::{StreamExt, future};
use mx4_core::Haptics;
use mx4_core::haptics::waveform;
use serde::{Deserialize, Serialize};
use tracing::info;
use zbus::message::Type;
use zbus::zvariant::{Structure, Value};
use zbus::{Connection, MatchRule, Message, MessageStream};

pub const EXTENSION_IFACE: &str = "com.mx4haptic.CursorTracker";
pub const EXTENSION_PATH: &str = "/com/mx4haptic/CursorTracker";
const NOTIFY_IFACE: &str = "org.freedesktop.Notifications";

/// The `[haptic]` config section. mx4d fills missing keys from [`Config::default`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub enabled: bool,
    pub display_boundary: TriggerConfig,
    pub notifications: TriggerConfig,
    pub workspace_switch: TriggerConfig,
    pub window_snap: TriggerConfig,
    pub focus_change: TriggerConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerConfig {
    pub enabled: bool,
    pub waveform: String,
    /// Events closer together than this play only once.
    pub cooldown_ms: u64,
}

impl TriggerConfig {
    fn new(waveform: &str, cooldown_ms: u64) -> Self {
        Self {
            enabled: true,
            waveform: waveform.into(),
            cooldown_ms,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            display_boundary: TriggerConfig::new("SUBTLE_COLLISION", 100),
            notifications: TriggerConfig::new("HAPPY_ALERT", 2000),
            workspace_switch: TriggerConfig::new("SUBTLE_COLLISION", 0),
            window_snap: TriggerConfig::new("SUBTLE_COLLISION", 300),
            focus_change: TriggerConfig::new("SUBTLE_COLLISION", 300),
        }
    }
}

impl Config {
    /// (config key, extension signal, trigger) for each extension trigger.
    fn extension_triggers(&self) -> [(&'static str, &'static str, &TriggerConfig); 4] {
        [
            (
                "display_boundary",
                "BoundaryCrossed",
                &self.display_boundary,
            ),
            (
                "workspace_switch",
                "WorkspaceSwitched",
                &self.workspace_switch,
            ),
            ("window_snap", "WindowSnapped", &self.window_snap),
            ("focus_change", "FocusChanged", &self.focus_change),
        ]
    }

    pub fn validate(&self) -> Result<()> {
        let all = self.extension_triggers().map(|(k, _, t)| (k, t));
        for (key, t) in all
            .into_iter()
            .chain([("notifications", &self.notifications)])
        {
            if waveform(&t.waveform).is_none() {
                bail!("haptic.{key}: unknown waveform {:?}", t.waveform);
            }
        }
        Ok(())
    }
}

/// Plays a waveform per event, dropping events inside the cooldown.
pub struct Trigger {
    name: &'static str,
    waveform: String,
    cooldown: Duration,
    last: Option<Instant>,
}

impl Trigger {
    pub fn new(name: &'static str, cfg: &TriggerConfig) -> Self {
        Self {
            name,
            waveform: cfg.waveform.clone(),
            cooldown: Duration::from_millis(cfg.cooldown_ms),
            last: None,
        }
    }

    /// Returns the waveform to play, or None if the event falls in the cooldown.
    pub fn fire(&mut self, now: Instant, detail: &str) -> Option<&str> {
        if self
            .last
            .is_some_and(|last| now.duration_since(last) < self.cooldown)
        {
            return None;
        }
        self.last = Some(now);
        info!("{}: {detail}", self.name);
        Some(&self.waveform)
    }
}

/// Subscribe the enabled triggers and play haptics until the bus goes away.
pub async fn run(haptics: Haptics, cfg: Config) -> Result<()> {
    let enabled: Vec<_> = cfg
        .extension_triggers()
        .into_iter()
        .chain([("notifications", "Notify", &cfg.notifications)])
        .filter(|(_, _, t)| t.enabled)
        .collect();
    info!(
        "Triggers enabled: {}",
        enabled
            .iter()
            .map(|(k, _, _)| *k)
            .collect::<Vec<_>>()
            .join(", ")
    );

    let signals: Vec<Trigger> = cfg
        .extension_triggers()
        .into_iter()
        .filter(|(_, _, t)| t.enabled)
        .map(|(_, signal, t)| Trigger::new(signal, t))
        .collect();
    let extension = watch_extension(haptics.clone(), signals);
    let notifications = async {
        if cfg.notifications.enabled {
            watch_notifications(haptics.clone(), Trigger::new("Notify", &cfg.notifications)).await
        } else {
            Ok(())
        }
    };
    future::try_join(extension, notifications).await?;
    Ok(())
}

async fn watch_extension(haptics: Haptics, mut triggers: Vec<Trigger>) -> Result<()> {
    if triggers.is_empty() {
        return Ok(());
    }
    let conn = Connection::session().await.context("session bus")?;
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .interface(EXTENSION_IFACE)?
        .path(EXTENSION_PATH)?
        .build();
    let mut stream = MessageStream::for_match_rule(rule, &conn, None).await?;
    while let Some(msg) = stream.next().await {
        let msg = msg?;
        let header = msg.header();
        let Some(member) = header.member() else {
            continue;
        };
        if let Some(t) = triggers.iter_mut().find(|t| t.name == member.as_str())
            && let Some(w) = t.fire(Instant::now(), &args(&msg))
        {
            haptics.play(w);
        }
    }
    Ok(())
}

/// Watch Notify() calls from any app. The daemon becomes a bus monitor on its
/// own connection, which sees method calls addressed to the notification server.
async fn watch_notifications(haptics: Haptics, mut trigger: Trigger) -> Result<()> {
    let conn = zbus::connection::Builder::session()?
        .build()
        .await
        .context("session bus")?;
    let rule = MatchRule::builder()
        .msg_type(Type::MethodCall)
        .interface(NOTIFY_IFACE)?
        .member("Notify")?
        .build();
    zbus::fdo::MonitoringProxy::new(&conn)
        .await?
        .become_monitor(&[rule], 0)
        .await
        .context("becoming a D-Bus monitor for notifications")?;
    let mut stream = MessageStream::from(&conn);
    while let Some(msg) = stream.next().await {
        let msg = msg?;
        let header = msg.header();
        if header.interface().map(|i| i.as_str()) != Some(NOTIFY_IFACE)
            || header.member().map(|m| m.as_str()) != Some("Notify")
        {
            continue;
        }
        let summary = fields(&msg)
            .into_iter()
            .nth(3)
            .unwrap_or_else(|| "?".into());
        if let Some(w) = trigger.fire(Instant::now(), &summary) {
            haptics.play(w);
        }
    }
    Ok(())
}

fn fields(msg: &Message) -> Vec<String> {
    let body = msg.body();
    let Ok(body) = body.deserialize::<Structure>() else {
        return Vec::new();
    };
    body.fields()
        .iter()
        .map(|v| match v {
            Value::Str(s) => s.to_string(),
            other => other.to_string(),
        })
        .collect()
}

fn args(msg: &Message) -> String {
    fields(msg).join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cooldown_drops_events_inside_window() {
        let mut t = Trigger::new("FocusChanged", &TriggerConfig::new("SUBTLE_COLLISION", 300));
        let t0 = Instant::now();
        assert!(t.fire(t0, "a").is_some());
        assert!(t.fire(t0 + Duration::from_millis(100), "b").is_none());
        assert!(t.fire(t0 + Duration::from_millis(400), "c").is_some());
    }

    #[test]
    fn zero_cooldown_plays_every_event() {
        let mut t = Trigger::new("WorkspaceSwitched", &TriggerConfig::new("KNOCK", 0));
        let t0 = Instant::now();
        assert_eq!(t.fire(t0, "").map(str::to_owned), Some("KNOCK".into()));
        assert!(t.fire(t0, "").is_some());
    }

    #[test]
    fn unknown_waveform_is_rejected() {
        let mut cfg = Config::default();
        cfg.window_snap.waveform = "NOPE".into();
        assert!(
            cfg.validate()
                .unwrap_err()
                .to_string()
                .contains("window_snap")
        );
        assert!(Config::default().validate().is_ok());
    }
}
