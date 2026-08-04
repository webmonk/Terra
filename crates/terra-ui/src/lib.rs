//! Editor chrome via terra-gui — IDE shell matching the Terra reference.

mod bookmarks_gui;
mod chrome_gui;
mod command_palette;
mod command_registry;
mod dock_gui;
mod history_gui;
mod inspector_gui;
mod inspector_schema;
mod layers_gui;
mod minimap_gui;
mod panels;
mod pipeline_gui;
mod presets;
mod quick_add;
mod thumbnails;
mod tool_catalog;
mod tools_gui;
mod viewport_gui;
mod workspace;

pub use chrome_gui::ChromeGuiState;
pub use command_palette::{draw_command_palette, CommandPaletteState, PaletteAction};
pub use command_registry::{commands, fuzzy_match, CommandCategory, CommandDef, CommandId};
pub use dock_gui::{DockGuiState, DockTab};
pub use inspector_gui::{draw_inspector_gui, InspectorGuiState};
pub use inspector_schema::InspectorSection;
pub use layers_gui::{draw_layers_gui, LayersGuiState};
pub use panels::{draw_windows, PanelAction, WindowsGuiState};
pub use presets::{
    builtin_presets, contextual_presets, layers_from_preset, ContextualPreset, LayerPreset,
};
pub use quick_add::{draw_quick_add, QuickAddState};
pub use tool_catalog::{all_tools, quick_add_entries, tools_for_mode, ToolAction, ToolDef};
pub use tools_gui::{draw_tools_gui, LayerToolDrag, ToolsGuiState};
pub use workspace::{AppWorkspace, WorkspaceMode};

use terra_core::document::TerrainDocument;
use terra_core::eval::PreviewQuality;
use terra_core::layer::{LayerId, LayerKind};
use terra_core::mask::MaskId;
use terra_gui::GuiContext;

use crate::chrome_gui::{draw_menu_bar, draw_menu_overlays};
use crate::dock_gui::draw_bottom_dock;
use crate::minimap_gui::draw_minimap;
use crate::viewport_gui::draw_viewport_overlays;

#[derive(Default)]
pub struct UiState {
    pub show_mask_editor: bool,
    pub show_content_browser: bool,
    pub show_export: bool,
    pub show_2d_preview: bool,
    pub show_profiler: bool,
    pub show_widget_lab: bool,
    pub preview_mode: Preview2dMode,
    pub editor_tool: EditorTool,
    /// High-level tool mode (Generate / Sculpt / …).
    pub workspace_mode: WorkspaceMode,
    /// Application workspace tab (Terrain / Materials / …).
    pub app_workspace: AppWorkspace,
    /// When false, inspector shows a short Simple param set (default).
    pub inspector_advanced: bool,
    /// Sculpt brush radius in normalized UV (Base raise/lower/smooth).
    pub sculpt_radius: f32,
    /// Raise/lower peak meters per stamp, or smooth blend 0–1.
    pub sculpt_strength: f32,
    /// Brush edge hardness: 0 is soft and 1 is hard.
    pub brush_falloff: f32,
    /// Fraction of the radius travelled between brush stamps.
    pub brush_spacing: f32,
    /// Continuous paint multiplier used by brush tools.
    pub brush_flow: f32,
    /// Reverse the active brush operation while painting.
    pub invert_brush: bool,
    /// Visibility switches for non-destructive viewport helpers.
    pub viewport_overlays: ViewportOverlayFlags,
    /// Dragging a layer tool from the left strip toward the viewport.
    pub tool_drag: Option<LayerToolDrag>,
    /// RGBA pixels supplied by the app from the latest successful evaluation.
    pub preview_rgba: Option<(u32, u32, Vec<u8>)>,
    pub export_path: Option<String>,
    /// `Some(0..=1)` while a background export is running.
    pub export_progress: Option<f32>,
    pub status: String,
    pub refining: bool,
    /// Best-effort progressive-build completion, from 0.0 through 1.0.
    pub build_progress: Option<f32>,
    /// Layer or pass currently being refined, when the evaluator can identify one.
    pub refining_layer_name: Option<String>,
    /// True when the visible terrain is a Draft or Medium evaluation.
    pub draft_displayed: bool,
    pub quality: PreviewQuality,
    pub profile: FrameProfile,
    pub selected_mask: Option<MaskId>,
    /// Active viewport mask paint target; `None` when sculpting Base or camera-free.
    pub paint_mask: Option<MaskId>,
    /// When true (Shift held), layer click opens context menu.
    pub shift_context: bool,
    /// Quick Add popup open.
    pub show_quick_add: bool,
    /// Most recently selected Quick Add tool IDs, newest first.
    pub recent_tools: Vec<String>,
    pub quick_add: QuickAddState,
    /// Command palette open.
    pub show_command_palette: bool,
    pub command_palette: CommandPaletteState,
    /// History panel open.
    pub show_history: bool,
    /// Pipeline overview open.
    pub show_pipeline: bool,
    /// Minimap visible.
    pub show_minimap: bool,
    /// Current camera look target expressed in terrain UV coordinates.
    pub camera_xz: (f32, f32),
    /// Current orbit-camera yaw in radians.
    pub camera_yaw: f32,
    /// Camera locations saved with Ctrl+1 through Ctrl+9.
    pub bookmarks: [Option<CameraBookmark>; 9],
    /// Read-only history labels supplied by the app, oldest to newest.
    pub history_descriptions: Vec<String>,
    /// Pending preview resolution chosen from the top-bar dropdown.
    pub pending_preview_resolution: Option<u32>,
    /// Environment lighting preset for the 3D viewport (presentation only).
    pub lighting_preset: LightingPreset,
    /// Viewport display "More" menu open.
    pub viewport_more_open: bool,
    /// Lighting preset menu open.
    pub lighting_menu_open: bool,
    /// Tool search query in the contextual tool panel.
    pub tool_search: String,
    /// Brush symmetry toggle (presentation / future stroke mirror).
    pub brush_symmetry: bool,
    /// Dock layout (resizable / collapsible panels).
    pub layout: terra_gui::LayoutPrefs,
    /// Bookmarks floating window visible.
    pub show_bookmarks: bool,
    /// Layer-kind clipboard for Copy/Paste Settings in the inspector.
    pub settings_clipboard: Option<LayerKind>,
    /// Pending camera bookmark save slot (0..8), consumed by the app.
    pub pending_bookmark_save: Option<usize>,
    /// Pending camera bookmark recall slot (0..8), consumed by the app.
    pub pending_bookmark_recall: Option<usize>,
    /// Dirty flag — layout should be written to disk.
    pub layout_dirty: bool,
}

/// Presentation-only environment lighting for the 3D viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LightingPreset {
    Studio,
    #[default]
    Midday,
    Sunset,
    Overcast,
    HighContrast,
    Neutral,
}

impl LightingPreset {
    pub const ALL: [LightingPreset; 6] = [
        LightingPreset::Studio,
        LightingPreset::Midday,
        LightingPreset::Sunset,
        LightingPreset::Overcast,
        LightingPreset::HighContrast,
        LightingPreset::Neutral,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LightingPreset::Studio => "Studio",
            LightingPreset::Midday => "Midday",
            LightingPreset::Sunset => "Sunset",
            LightingPreset::Overcast => "Overcast",
            LightingPreset::HighContrast => "High Contrast",
            LightingPreset::Neutral => "Neutral",
        }
    }

    /// Returns (light_dir_xyz + intensity_w, exposure, clear_rgb).
    pub fn params(self) -> ([f32; 4], f32, [f32; 3]) {
        match self {
            LightingPreset::Studio => ([-0.25, -0.95, -0.15, 1.05], 1.05, [0.18, 0.20, 0.24]),
            LightingPreset::Midday => ([-0.40, -0.88, -0.25, 1.15], 1.10, [0.42, 0.52, 0.62]),
            LightingPreset::Sunset => ([-0.75, -0.45, -0.20, 1.20], 1.15, [0.55, 0.38, 0.32]),
            LightingPreset::Overcast => ([-0.15, -0.98, -0.10, 0.75], 1.00, [0.45, 0.48, 0.52]),
            LightingPreset::HighContrast => ([-0.55, -0.75, -0.35, 1.35], 1.05, [0.12, 0.14, 0.18]),
            LightingPreset::Neutral => ([-0.35, -0.90, -0.20, 1.00], 1.00, [0.28, 0.32, 0.38]),
        }
    }
}

/// Persisted orbit-camera state for a numbered navigation bookmark.
#[derive(Debug, Clone, Copy)]
pub struct CameraBookmark {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

impl UiState {
    pub fn ensure_sculpt_defaults(&mut self) {
        if self.sculpt_radius <= 0.0 {
            self.sculpt_radius = 0.04;
        }
        if self.sculpt_strength <= 0.0 {
            self.sculpt_strength = 4.0;
        }
        if self.brush_falloff <= 0.0 {
            self.brush_falloff = 0.5;
        }
        if self.brush_spacing <= 0.0 {
            self.brush_spacing = 0.1;
        }
        if self.brush_flow <= 0.0 {
            self.brush_flow = 1.0;
        }
    }
}

/// Optional viewport aids. These flags affect editor presentation, not terrain data.
#[derive(Debug, Clone, Copy, Default)]
pub struct ViewportOverlayFlags {
    pub grid: bool,
    pub world_bounds: bool,
    pub water_level: bool,
    pub contours: bool,
    pub wireframe: bool,
    pub brush_preview: bool,
    pub mask_overlay: bool,
    pub perf_info: bool,
}

/// Left-sidebar / sculpt tool selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditorTool {
    /// Photoshop-style hand: orbit / pan / zoom in the viewport.
    #[default]
    Move,
    Raise,
    Lower,
    Smooth,
    /// Mask paint (upper layers).
    PaintMask,
    /// Viewport diagnostic overlays (also set via viewport pills).
    Height,
    Slope,
    Flow,
    Masks,
}

impl EditorTool {
    pub fn as_preview_mode(self) -> Option<Preview2dMode> {
        match self {
            EditorTool::Height => Some(Preview2dMode::Height),
            EditorTool::Slope => Some(Preview2dMode::Slope),
            EditorTool::Flow => Some(Preview2dMode::Flow),
            EditorTool::Masks => Some(Preview2dMode::Masks),
            _ => None,
        }
    }

    pub fn is_move(self) -> bool {
        matches!(self, EditorTool::Move)
    }

    pub fn is_sculpt(self) -> bool {
        matches!(
            self,
            EditorTool::Raise | EditorTool::Lower | EditorTool::Smooth
        )
    }

    /// Raise / Lower / Smooth / Paint Mask — own left-drag paint and wheel size.
    pub fn is_brush(self) -> bool {
        self.is_sculpt() || matches!(self, EditorTool::PaintMask)
    }

    pub fn is_view(self) -> bool {
        self.as_preview_mode().is_some()
    }

    pub fn sculpt_mode(self) -> Option<u8> {
        match self {
            EditorTool::Raise => Some(0),
            EditorTool::Lower => Some(1),
            EditorTool::Smooth => Some(2),
            _ => None,
        }
    }
}

/// Per-frame timings in microseconds for the profiler overlay.
#[derive(Debug, Clone, Default)]
pub struct FrameProfile {
    pub eval_us: u64,
    pub upload_us: u64,
    pub render_us: u64,
    pub ui_us: u64,
    pub frame_us: u64,
    pub gen_id: u64,
    pub tex_w: u32,
    pub tex_h: u32,
    pub quality: &'static str,
    /// `"GPU"` preview engine or `"CPU"` fallback.
    pub path: &'static str,
    pub clipmap_levels: u32,
    pub tiles_x: u32,
    pub tiles_z: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Preview2dMode {
    #[default]
    Lit,
    Unlit,
    Height,
    Slope,
    Curvature,
    Flow,
    Convexity,
    Concavity,
    Normals,
    Wireframe,
    AmbientOcclusion,
    Material,
    Biome,
    /// Legacy singular spelling retained for integrations using the Phase 3 API.
    Mask,
    Masks,
    VegetationDensity,
}

#[derive(Debug, Default)]
pub struct FrameUiOutput {
    pub actions: Vec<PanelAction>,
    pub selected: Option<LayerId>,
    pub request_undo: bool,
    pub request_redo: bool,
    pub request_save_as: bool,
    pub request_load_path: bool,
    /// Pick an export directory without starting the job.
    pub request_export_path: bool,
    /// Start export to the current path (or pick one first if unset).
    pub request_start_export: bool,
    pub camera_reset: bool,
    pub camera_top_view: bool,
    pub camera_frame_selection: bool,
    /// Move the orbit target to a terrain-normalized location selected in the minimap.
    pub request_camera_focus_uv: Option<(f32, f32)>,
    /// Cancel the in-flight evaluation / refine job.
    pub request_cancel_build: bool,
    /// Save the current camera into the next free bookmark slot.
    pub request_save_bookmark: bool,
    /// Save into a specific bookmark slot (0..8).
    pub request_save_bookmark_slot: Option<usize>,
    /// Recall a bookmark slot (0..8).
    pub request_recall_bookmark: Option<usize>,
}

/// Draw all editor chrome with terra-gui.
pub fn draw_editor_gui(
    ui: &mut GuiContext<'_>,
    doc: &mut TerrainDocument,
    ui_state: &mut UiState,
    chrome: &mut ChromeGuiState,
    tools: &mut ToolsGuiState,
    layers: &mut LayersGuiState,
    inspector: &mut InspectorGuiState,
    windows: &mut WindowsGuiState,
    dock: &mut DockGuiState,
) -> FrameUiOutput {
    ui_state.ensure_sculpt_defaults();
    // Sync live layout into the GUI state used by rect helpers.
    ui.state.layout = ui_state.layout.clone();

    let mut out = FrameUiOutput {
        selected: doc.selected,
        ..Default::default()
    };

    draw_menu_bar(ui, ui_state, chrome, &mut out);
    out.actions.extend(draw_tools_gui(ui, doc, ui_state, tools));
    out.actions
        .extend(draw_layers_gui(ui, doc, ui_state, layers));
    out.actions
        .extend(draw_inspector_gui(ui, doc, ui_state, inspector));
    draw_viewport_overlays(ui, ui_state, &mut out);
    draw_minimap(ui, ui_state, &mut out);
    draw_bottom_dock(ui, doc, ui_state, dock, &mut out);
    draw_windows(ui, doc, ui_state, windows, &mut out);
    pipeline_gui::draw_pipeline_gui(
        ui,
        doc,
        ui_state,
        &mut windows.pipeline_scroll,
        &mut out.actions,
    );
    history_gui::draw_history_gui(ui, ui_state, &mut windows.history_scroll, &mut out);
    bookmarks_gui::draw_bookmarks_gui(ui, ui_state, &mut windows.bookmarks_scroll, &mut out);

    if ui.draw_layout_splitters() {
        ui_state.layout = ui.state.layout.clone();
        ui_state.layout_dirty = true;
    }

    // Menus last so they stack above panels and receive hit-testing.
    draw_menu_overlays(ui, ui_state, chrome, &mut out);
    // Global popups are drawn after menus and other chrome.
    let mut quick_add = std::mem::take(&mut ui_state.quick_add);
    out.actions
        .extend(draw_quick_add(ui, ui_state, &mut quick_add));
    ui_state.quick_add = quick_add;
    let mut command_palette = std::mem::take(&mut ui_state.command_palette);
    for action in draw_command_palette(ui, ui_state, &mut command_palette) {
        match action {
            PaletteAction::Panel(action) => out.actions.push(action),
            PaletteAction::Undo => out.request_undo = true,
            PaletteAction::Redo => out.request_redo = true,
            PaletteAction::Export => ui_state.show_export = true,
            PaletteAction::CameraReset => out.camera_reset = true,
            PaletteAction::CameraTopView => out.camera_top_view = true,
            PaletteAction::CameraFrameSelection => out.camera_frame_selection = true,
        }
    }
    ui_state.command_palette = command_palette;
    out.selected = doc.selected;
    out
}
