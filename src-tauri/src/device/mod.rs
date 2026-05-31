#![allow(dead_code)]

pub mod fel;
pub mod usb;

use serde::Serialize;

/// SoC identity read from the FEL version handshake.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SocInfo {
    pub soc_id: u16,
    pub name: String,
}

/// Live device state reported to the frontend.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum DeviceStatus {
    Disconnected,
    DetectedNoDriver,
    Connected { soc: SocInfo },
}

/// Outcome of a single probe attempt against the USB bus.
#[derive(Debug, Clone, PartialEq)]
pub enum ProbeOutcome {
    Absent,
    DetectedNoDriver,
    Connected(SocInfo),
}

impl From<ProbeOutcome> for DeviceStatus {
    fn from(o: ProbeOutcome) -> Self {
        match o {
            ProbeOutcome::Absent => DeviceStatus::Disconnected,
            ProbeOutcome::DetectedNoDriver => DeviceStatus::DetectedNoDriver,
            ProbeOutcome::Connected(soc) => DeviceStatus::Connected { soc },
        }
    }
}

/// Probes the bus for a FEL device. Real impl talks to rusb; tests use a mock.
pub trait DeviceProbe {
    fn probe(&self) -> ProbeOutcome;
}

/// Detects status transitions so the poll loop only emits on change.
pub struct Monitor<P: DeviceProbe> {
    probe: P,
    last: Option<DeviceStatus>,
}

impl<P: DeviceProbe> Monitor<P> {
    pub fn new(probe: P) -> Self {
        Self { probe, last: None }
    }

    /// Returns `Some(status)` when the status changed since the previous tick, else `None`.
    pub fn tick(&mut self) -> Option<DeviceStatus> {
        let current: DeviceStatus = self.probe.probe().into();
        if self.last.as_ref() == Some(&current) {
            None
        } else {
            self.last = Some(current.clone());
            Some(current)
        }
    }

    /// The most recently observed status (Disconnected if never ticked).
    pub fn current(&self) -> DeviceStatus {
        self.last.clone().unwrap_or(DeviceStatus::Disconnected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct ScriptedProbe {
        script: Vec<ProbeOutcome>,
        idx: Cell<usize>,
    }
    impl DeviceProbe for ScriptedProbe {
        fn probe(&self) -> ProbeOutcome {
            let i = self.idx.get();
            self.idx.set(i + 1);
            self.script[i].clone()
        }
    }

    #[test]
    fn emits_only_on_transition() {
        let soc = SocInfo { soc_id: 0x1667, name: "Allwinner R16".into() };
        let probe = ScriptedProbe {
            script: vec![
                ProbeOutcome::Absent,
                ProbeOutcome::Connected(soc.clone()),
                ProbeOutcome::Connected(soc.clone()),
                ProbeOutcome::Absent,
            ],
            idx: Cell::new(0),
        };
        let mut m = Monitor::new(probe);
        assert_eq!(m.tick(), Some(DeviceStatus::Disconnected));
        assert_eq!(m.tick(), Some(DeviceStatus::Connected { soc: soc.clone() }));
        assert_eq!(m.tick(), None);
        assert_eq!(m.tick(), Some(DeviceStatus::Disconnected));
    }
}
