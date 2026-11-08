//! Monitor identity and monitor-local selection geometry. Desktop origins are
//! physical pixels; only local coordinates are multiplied by the monitor DPI.

use crate::config::CaptureRegion;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CaptureMonitor {
    pub device_id: String,
    pub bounds: CaptureRegion,
    pub scale_factor: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CaptureSelection {
    pub region: CaptureRegion,
    pub monitor: CaptureMonitor,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TargetError {
    #[error("The display changed. Select the subtitle area again.")]
    DisplayChanged,
    #[error("Select a subtitle area on the screen containing the app window.")]
    SelectOnCurrentDisplay,
    #[error("The subtitle area must fit inside one screen.")]
    OutsideMonitor,
    #[error("This selection has expired. Open the area selector again.")]
    ExpiredSelection,
}

impl CaptureMonitor {
    pub fn desktop_region(&self, local: CaptureRegion) -> Result<CaptureRegion, TargetError> {
        if !local.is_valid()
            || local.x < 0
            || local.y < 0
            || local.x as i64 + local.width as i64 > self.bounds.width as i64
            || local.y as i64 + local.height as i64 > self.bounds.height as i64
        {
            return Err(TargetError::OutsideMonitor);
        }
        Ok(CaptureRegion::new(
            self.bounds
                .x
                .checked_add(local.x)
                .ok_or(TargetError::OutsideMonitor)?,
            self.bounds
                .y
                .checked_add(local.y)
                .ok_or(TargetError::OutsideMonitor)?,
            local.width,
            local.height,
        ))
    }

    pub fn contains_logical(&self, region: CaptureRegion) -> bool {
        self.bounds.is_valid()
            && !self.device_id.is_empty()
            && self.scale_factor.is_finite()
            && self.scale_factor > 0.0
            && region.is_valid()
            && region.x >= 0
            && region.y >= 0
            && (region.x as i64 + region.width as i64) as f64
                <= (self.bounds.width as f64 / self.scale_factor).ceil()
            && (region.y as i64 + region.height as i64) as f64
                <= (self.bounds.height as f64 / self.scale_factor).ceil()
    }

    /// The saved identity, bounds and DPI must still describe the same surface.
    /// Reconnection never resumes an interrupted translation session.
    pub fn is_present(&self, monitors: &[Self]) -> bool {
        monitors.iter().any(|monitor| monitor == self)
    }
}

impl CaptureSelection {
    pub fn validate_start(
        &self,
        current: &CaptureMonitor,
        monitors: &[CaptureMonitor],
    ) -> Result<(), TargetError> {
        if !self.monitor.is_present(monitors) {
            return Err(TargetError::DisplayChanged);
        }
        if &self.monitor != current {
            return Err(TargetError::SelectOnCurrentDisplay);
        }
        self.physical_local().map(|_| ())
    }

    pub fn new(region: CaptureRegion, monitor: CaptureMonitor) -> Result<Self, TargetError> {
        if !monitor.contains_logical(region) {
            return Err(TargetError::OutsideMonitor);
        }
        let selection = Self { region, monitor };
        selection.physical_local()?;
        Ok(selection)
    }

    pub fn physical_local(&self) -> Result<CaptureRegion, TargetError> {
        if !self.monitor.contains_logical(self.region) {
            return Err(TargetError::OutsideMonitor);
        }
        let scale = self.monitor.scale_factor;
        let x = (self.region.x as f64 * scale).round() as i32;
        let y = (self.region.y as f64 * scale).round() as i32;
        // Round edges together. A CSS viewport can round up a fractional pixel;
        // trim that edge without shifting the selected content.
        let right = (((self.region.x as i64 + self.region.width as i64) as f64 * scale).round()
            as i32)
            .min(self.monitor.bounds.width);
        let bottom = (((self.region.y as i64 + self.region.height as i64) as f64 * scale).round()
            as i32)
            .min(self.monitor.bounds.height);
        let result = CaptureRegion::new(x, y, right - x, bottom - y);
        result
            .is_valid()
            .then_some(result)
            .ok_or(TargetError::OutsideMonitor)
    }
}

#[derive(Default)]
pub struct CaptureState {
    pub selected: Option<CaptureSelection>,
    pending: Option<(u64, CaptureMonitor)>,
    next_selection_id: u64,
}

impl CaptureState {
    pub fn begin_selection(&mut self, monitor: CaptureMonitor) -> u64 {
        self.next_selection_id = self.next_selection_id.wrapping_add(1);
        self.pending = Some((self.next_selection_id, monitor));
        self.next_selection_id
    }

    pub fn cancel_selection(&mut self) {
        self.pending = None;
    }

    pub fn confirm_native(
        &mut self,
        physical: CaptureRegion,
        monitors: &[CaptureMonitor],
    ) -> Result<CaptureSelection, TargetError> {
        let (id, monitor) = self.pending.clone().ok_or(TargetError::ExpiredSelection)?;
        let scale = monitor.scale_factor;
        let region = CaptureRegion::new(
            ((physical.x as f64 - monitor.bounds.x as f64) / scale).round() as i32,
            ((physical.y as f64 - monitor.bounds.y as f64) / scale).round() as i32,
            (physical.width as f64 / scale).round() as i32,
            (physical.height as f64 / scale).round() as i32,
        );
        self.confirm(id, region, monitors)
    }

    pub fn confirm(
        &mut self,
        selection_id: u64,
        region: CaptureRegion,
        monitors: &[CaptureMonitor],
    ) -> Result<CaptureSelection, TargetError> {
        let selection = self.selection_to_confirm(selection_id, region, monitors)?;
        self.selected = Some(selection.clone());
        Ok(selection)
    }

    pub fn selection_to_confirm(
        &self,
        selection_id: u64,
        region: CaptureRegion,
        monitors: &[CaptureMonitor],
    ) -> Result<CaptureSelection, TargetError> {
        let (id, monitor) = self.pending.as_ref().ok_or(TargetError::ExpiredSelection)?;
        if *id != selection_id {
            return Err(TargetError::ExpiredSelection);
        }
        if !monitor.is_present(monitors) {
            return Err(TargetError::DisplayChanged);
        }
        CaptureSelection::new(region, monitor.clone())
    }

    pub fn visible_region(
        &self,
        current: &CaptureMonitor,
        running: bool,
        monitors: &[CaptureMonitor],
    ) -> Option<CaptureRegion> {
        self.selected
            .as_ref()
            .filter(|selected| {
                selected.monitor.is_present(monitors) && (running || &selected.monitor == current)
            })
            .map(|s| s.region)
    }

    pub fn restore(
        &mut self,
        region: Option<CaptureRegion>,
        saved_monitor: Option<CaptureMonitor>,
        old_scale: Option<f64>,
        monitors: &[CaptureMonitor],
    ) {
        let monitor = saved_monitor.or_else(|| {
            // Old configs described the primary monitor only. A multi-screen
            // desktop gives no evidence of which physical screen that was.
            (monitors.len() == 1)
                .then(|| monitors[0].clone())
                .filter(|monitor| old_scale.is_none_or(|scale| scale == monitor.scale_factor))
        });
        self.selected = region
            .zip(monitor)
            .filter(|(_, monitor)| monitor.is_present(monitors))
            .and_then(|(region, monitor)| CaptureSelection::new(region, monitor).ok());
    }
}

#[cfg(test)]
#[path = "capture_target_tests.rs"]
mod tests;
