//! Runtime editor chrome layout (resizable panels + collapse flags).

use serde::{Deserialize, Serialize};

use crate::style::{MODE_RAIL_W, RIGHT_PANEL_W, TOOL_PANEL_W};

/// Persisted / live dock layout for the editor shell.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutPrefs {
    /// Contextual tool panel width (logical px). Ignored when collapsed.
    pub tool_panel_w: f32,
    /// Right rail (layers + inspector) width.
    pub right_panel_w: f32,
    /// Fraction of the right rail height for the layers stack (0–1).
    pub layers_frac: f32,
    /// When true, the contextual tool panel is hidden (mode rail remains).
    pub tool_panel_collapsed: bool,
    /// When true, the inspector is collapsed and layers take the full right rail.
    pub inspector_collapsed: bool,
}

impl Default for LayoutPrefs {
    fn default() -> Self {
        Self {
            tool_panel_w: TOOL_PANEL_W,
            right_panel_w: RIGHT_PANEL_W,
            layers_frac: 0.52,
            tool_panel_collapsed: false,
            inspector_collapsed: false,
        }
    }
}

impl LayoutPrefs {
    pub const TOOL_PANEL_MIN: f32 = 140.0;
    pub const TOOL_PANEL_MAX: f32 = 280.0;
    pub const RIGHT_PANEL_MIN: f32 = 280.0;
    pub const RIGHT_PANEL_MAX: f32 = 520.0;
    pub const LAYERS_FRAC_MIN: f32 = 0.25;
    pub const LAYERS_FRAC_MAX: f32 = 0.80;

    pub fn clamp_mut(&mut self) {
        self.tool_panel_w = self
            .tool_panel_w
            .clamp(Self::TOOL_PANEL_MIN, Self::TOOL_PANEL_MAX);
        self.right_panel_w = self
            .right_panel_w
            .clamp(Self::RIGHT_PANEL_MIN, Self::RIGHT_PANEL_MAX);
        self.layers_frac = self
            .layers_frac
            .clamp(Self::LAYERS_FRAC_MIN, Self::LAYERS_FRAC_MAX);
    }

    pub fn effective_tool_panel_w(&self) -> f32 {
        if self.tool_panel_collapsed {
            0.0
        } else {
            self.tool_panel_w.clamp(Self::TOOL_PANEL_MIN, Self::TOOL_PANEL_MAX)
        }
    }

    pub fn left_chrome_w(&self) -> f32 {
        MODE_RAIL_W + self.effective_tool_panel_w()
    }

    pub fn effective_right_w(&self) -> f32 {
        self.right_panel_w
            .clamp(Self::RIGHT_PANEL_MIN, Self::RIGHT_PANEL_MAX)
    }

    pub fn effective_layers_frac(&self) -> f32 {
        if self.inspector_collapsed {
            1.0
        } else {
            self.layers_frac
                .clamp(Self::LAYERS_FRAC_MIN, Self::LAYERS_FRAC_MAX)
        }
    }

    /// Reset to design-token defaults.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Splitter hit thickness in logical px.
pub const SPLITTER_HIT: f32 = 5.0;
