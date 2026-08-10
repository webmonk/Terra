//! Runtime dock layout (resizable panels + collapse flags).
//!
//! Defaults are product-neutral; apps may override via persisted prefs.

use serde::{Deserialize, Serialize};

use crate::style::RIGHT_PANEL_W;

/// Default left rail width (mode/navigation column) for IDE-style shells.
const DEFAULT_LEFT_RAIL_W: f32 = 72.0;
/// Default contextual tool panel width.
const DEFAULT_TOOL_PANEL_W: f32 = 228.0;
/// Default full-height left dock (e.g. mask editor).
const DEFAULT_LEFT_DOCK_W: f32 = 320.0;

/// Persisted / live dock layout for an IDE-style shell.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutPrefs {
    /// Contextual tool panel width (logical px). Ignored when collapsed.
    pub tool_panel_w: f32,
    /// Right rail (layers + inspector) width.
    pub right_panel_w: f32,
    /// Fraction of the right rail height for the layers stack (0–1).
    pub layers_frac: f32,
    /// When true, the contextual tool panel is hidden (mode rail remains when visible).
    pub tool_panel_collapsed: bool,
    /// When true, the inspector is collapsed and layers take the full right rail.
    pub inspector_collapsed: bool,
    /// Runtime: hide Generate/Sculpt mode rail (artist intents). Not persisted meaningfully.
    #[serde(default)]
    pub hide_mode_rail: bool,
    /// Runtime: hide WC layers tree (artist intents). Not persisted meaningfully.
    #[serde(default)]
    pub hide_layers_panel: bool,
    /// Width of the Mask-view left dock. Persisted across sessions.
    #[serde(default = "default_mask_editor_panel_w")]
    pub mask_editor_panel_w: f32,
    /// Runtime: when set, left chrome is a single full-height dock of this width
    /// (mode rail + tool panel hidden). Used by Mask view.
    #[serde(default, skip_serializing)]
    pub left_dock_w: Option<f32>,
    /// Preferred task workspace id (`sculpt`, `biomes`, …). Editor preference only.
    #[serde(default = "default_preferred_workspace")]
    pub preferred_workspace: String,
    /// When true, creating an entity may switch to its home workspace.
    /// Default false — artists stay in the current workspace unless they opt in.
    #[serde(default)]
    pub auto_switch_workspace_on_create: bool,
    /// Persisted viewport render quality settings (user preference).
    #[serde(default)]
    pub viewport_render: ViewportRenderPrefs,
}

/// User-level viewport render preferences (Phase 12).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewportRenderPrefs {
    #[serde(default = "default_render_mode")]
    pub mode: String,
    #[serde(default = "default_render_preset")]
    pub preset: String,
    #[serde(default = "default_target_fps")]
    pub target_fps: f32,
    #[serde(default = "default_max_spp")]
    pub max_spp: u32,
    #[serde(default = "default_true")]
    pub dynamic_resolution: bool,
    #[serde(default = "default_true")]
    pub denoise: bool,
    #[serde(default = "default_interactive_spp")]
    pub interactive_spp: u32,
    #[serde(default = "default_settling_spp")]
    pub settling_spp: u32,
    #[serde(default = "default_refining_spp")]
    pub refining_spp: u32,
    #[serde(default = "default_bounces_interactive")]
    pub max_bounces_interactive: u32,
    #[serde(default = "default_bounces_refining")]
    pub max_bounces_refining: u32,
    #[serde(default = "default_min_scale")]
    pub min_internal_scale: f32,
    #[serde(default = "default_max_scale")]
    pub max_internal_scale: f32,
    #[serde(default = "default_history_clamp")]
    pub history_clamp_k: f32,
    #[serde(default = "default_converge")]
    pub converge_fraction: f32,
    #[serde(default)]
    pub debug_viz_mode: u32,
}

fn default_render_mode() -> String {
    "Raster".into()
}
fn default_render_preset() -> String {
    "Balanced".into()
}
fn default_target_fps() -> f32 {
    60.0
}
fn default_max_spp() -> u32 {
    128
}
fn default_true() -> bool {
    true
}
fn default_interactive_spp() -> u32 {
    1
}
fn default_settling_spp() -> u32 {
    2
}
fn default_refining_spp() -> u32 {
    4
}
fn default_bounces_interactive() -> u32 {
    2
}
fn default_bounces_refining() -> u32 {
    4
}
fn default_min_scale() -> f32 {
    0.5
}
fn default_max_scale() -> f32 {
    1.0
}
fn default_history_clamp() -> f32 {
    1.5
}
fn default_converge() -> f32 {
    0.95
}

impl Default for ViewportRenderPrefs {
    fn default() -> Self {
        Self {
            mode: default_render_mode(),
            preset: default_render_preset(),
            target_fps: default_target_fps(),
            max_spp: default_max_spp(),
            dynamic_resolution: true,
            denoise: true,
            interactive_spp: default_interactive_spp(),
            settling_spp: default_settling_spp(),
            refining_spp: default_refining_spp(),
            max_bounces_interactive: default_bounces_interactive(),
            max_bounces_refining: default_bounces_refining(),
            min_internal_scale: default_min_scale(),
            max_internal_scale: default_max_scale(),
            history_clamp_k: default_history_clamp(),
            converge_fraction: default_converge(),
            debug_viz_mode: 0,
        }
    }
}

fn default_preferred_workspace() -> String {
    "sculpt".into()
}

fn default_mask_editor_panel_w() -> f32 {
    DEFAULT_LEFT_DOCK_W
}

impl Default for LayoutPrefs {
    fn default() -> Self {
        Self {
            tool_panel_w: DEFAULT_TOOL_PANEL_W,
            right_panel_w: RIGHT_PANEL_W,
            layers_frac: 0.52,
            tool_panel_collapsed: false,
            inspector_collapsed: false,
            hide_mode_rail: false,
            hide_layers_panel: false,
            mask_editor_panel_w: DEFAULT_LEFT_DOCK_W,
            left_dock_w: None,
            preferred_workspace: default_preferred_workspace(),
            auto_switch_workspace_on_create: false,
            viewport_render: ViewportRenderPrefs::default(),
        }
    }
}

impl LayoutPrefs {
    pub const TOOL_PANEL_MIN: f32 = 140.0;
    pub const TOOL_PANEL_MAX: f32 = 280.0;
    pub const MASK_EDITOR_MIN: f32 = 260.0;
    pub const MASK_EDITOR_MAX: f32 = 420.0;
    pub const RIGHT_PANEL_MIN: f32 = 280.0;
    pub const RIGHT_PANEL_MAX: f32 = 520.0;
    pub const LAYERS_FRAC_MIN: f32 = 0.25;
    pub const LAYERS_FRAC_MAX: f32 = 0.80;

    pub fn clamp_mut(&mut self) {
        self.tool_panel_w = self
            .tool_panel_w
            .clamp(Self::TOOL_PANEL_MIN, Self::TOOL_PANEL_MAX);
        self.mask_editor_panel_w = self
            .mask_editor_panel_w
            .clamp(Self::MASK_EDITOR_MIN, Self::MASK_EDITOR_MAX);
        if let Some(w) = self.left_dock_w.as_mut() {
            *w = w.clamp(Self::MASK_EDITOR_MIN, Self::MASK_EDITOR_MAX);
        }
        self.right_panel_w = self
            .right_panel_w
            .clamp(Self::RIGHT_PANEL_MIN, Self::RIGHT_PANEL_MAX);
        self.layers_frac = self
            .layers_frac
            .clamp(Self::LAYERS_FRAC_MIN, Self::LAYERS_FRAC_MAX);
    }

    pub fn effective_tool_panel_w(&self) -> f32 {
        if self.left_dock_w.is_some() || self.tool_panel_collapsed {
            0.0
        } else {
            self.tool_panel_w
                .clamp(Self::TOOL_PANEL_MIN, Self::TOOL_PANEL_MAX)
        }
    }

    pub fn mode_rail_w(&self) -> f32 {
        if self.left_dock_w.is_some() || self.hide_mode_rail {
            0.0
        } else {
            DEFAULT_LEFT_RAIL_W
        }
    }

    pub fn left_chrome_w(&self) -> f32 {
        if let Some(w) = self.left_dock_w {
            return w.clamp(Self::MASK_EDITOR_MIN, Self::MASK_EDITOR_MAX);
        }
        self.mode_rail_w() + self.effective_tool_panel_w()
    }

    pub fn effective_right_w(&self) -> f32 {
        self.right_panel_w
            .clamp(Self::RIGHT_PANEL_MIN, Self::RIGHT_PANEL_MAX)
    }

    pub fn effective_layers_frac(&self) -> f32 {
        if self.hide_layers_panel {
            0.0
        } else if self.inspector_collapsed {
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
