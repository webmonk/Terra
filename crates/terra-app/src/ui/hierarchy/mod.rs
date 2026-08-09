//! Right-rail hierarchy over `doc.stack` (stack-only; product Regions removed).

use crate::ui::actions::PanelAction;
use crate::ui::hierarchy_view::{
    self, concept_for_category, concept_for_top_level, concept_row_id, ArtistConcept, TERRAIN_ROOT,
    TERRAIN_SCOPE_KEY,
};
use crate::ui::style::{
    self, FONT_SCALE, LAYER_ROW_H, PAD, TYPE_BODY, TYPE_LABEL,
};
use crate::ui::thumbnails::ThumbnailCache;
use crate::ui::{UiState, WorkspaceId};
use terra_core::document::TerrainDocument;
use terra_core::domain::DomainRole;
use terra_core::layer::{BlendMode, LayerId, LayerKind, StackNode};
use terra_core::mask::MaskId;
use terra_gui::{icon_button, Color, GuiContext, Icon, Id, Rect};

const LIST_INSET: f32 = 10.0;
const INDENT_STEP: f32 = 14.0;
const SEC_ICON_SZ: f32 = 14.0;
const SEC_ROW_H: f32 = 28.0;
const FOOTER_H: f32 = 36.0;
const FOOTER_BTN: f32 = 26.0;

pub(crate) const BLEND_MODES: [BlendMode; 12] = [
    BlendMode::Normal,
    BlendMode::Replace,
    BlendMode::Add,
    BlendMode::Subtract,
    BlendMode::Multiply,
    BlendMode::Min,
    BlendMode::Max,
    BlendMode::Interpolate,
    BlendMode::HeightBlend,
    BlendMode::Overlay,
    BlendMode::SmoothMaximum,
    BlendMode::SmoothMinimum,
];

pub(crate) const BLEND_LABELS: [&str; 12] = [
    "Normal",
    "Replace",
    "Add",
    "Subtract",
    "Multiply",
    "Min",
    "Max",
    "Interpolate",
    "Height Blend",
    "Overlay",
    "Smooth Max",
    "Smooth Min",
];

pub(crate) fn blend_mode_index(mode: BlendMode) -> usize {
    BLEND_MODES.iter().position(|b| *b == mode).unwrap_or(0)
}

pub(crate) fn blend_mode_at(idx: usize) -> BlendMode {
    BLEND_MODES[idx.min(BLEND_MODES.len() - 1)]
}

#[derive(Debug, Clone)]
pub struct LayerDragSource {
    pub id: LayerId,
    pub root_idx: usize,
    pub name: String,
    pub icon: Icon,
}

#[derive(Debug, Clone)]
pub struct LayerPresentationState {
    pub collapsed_groups: Vec<LayerId>,
    pub search_text: String,
    pub show_thumbnails: bool,
    pub hover_layer: Option<LayerId>,
    pub hide_disabled: bool,
    pub show_only_relevant: bool,
    pub inspector_tab: Option<String>,
}

impl Default for LayerPresentationState {
    fn default() -> Self {
        Self {
            collapsed_groups: Vec::new(),
            search_text: String::new(),
            show_thumbnails: true,
            hover_layer: None,
            hide_disabled: false,
            show_only_relevant: false,
            inspector_tab: None,
        }
    }
}

impl LayerPresentationState {
    pub fn toggle_collapsed(&mut self, id: LayerId) {
        if let Some(i) = self.collapsed_groups.iter().position(|x| *x == id) {
            self.collapsed_groups.remove(i);
        } else {
            self.collapsed_groups.push(id);
        }
    }

    pub fn is_collapsed(&self, id: LayerId) -> bool {
        self.collapsed_groups.contains(&id)
    }
}

#[derive(Debug)]
pub struct LayersGuiState {
    pub scroll_y: f32,
    pub add_menu_open: bool,
    pub drag_from: Option<LayerDragSource>,
    pub cursor_hint: crate::ui::UiCursor,
    pub context_menu: Option<(LayerId, f32, f32)>,
    pub rename_id: Option<LayerId>,
    pub presentation: LayerPresentationState,
    known_collapse_ids: Vec<LayerId>,
    pub thumbnails: ThumbnailCache,
}

impl Default for LayersGuiState {
    fn default() -> Self {
        Self {
            scroll_y: 0.0,
            add_menu_open: false,
            drag_from: None,
            cursor_hint: crate::ui::UiCursor::Default,
            context_menu: None,
            rename_id: None,
            presentation: LayerPresentationState::default(),
            known_collapse_ids: Vec::new(),
            thumbnails: ThumbnailCache::default(),
        }
    }
}

impl LayersGuiState {
    pub fn collapsed_groups(&self) -> &[LayerId] {
        &self.presentation.collapsed_groups
    }

    pub fn collapsed_groups_mut(&mut self) -> &mut Vec<LayerId> {
        &mut self.presentation.collapsed_groups
    }

    pub fn reset_collapse_for_project(&mut self, _doc: Option<&TerrainDocument>) {
        self.presentation.collapsed_groups.clear();
        self.known_collapse_ids.clear();
        self.scroll_y = 0.0;
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TreeRole {
    SectionLabel,
    Foundation,
    Biome,
    BiomeSection,
    Group,
    Layer,
    ConceptFolder,
    MaskAsset,
}

struct LayerRow {
    id: LayerId,
    name: String,
    enabled: bool,
    type_icon: Icon,
    depth: u8,
    role: TreeRole,
    domain_role: Option<DomainRole>,
    concept: Option<ArtistConcept>,
    select_as: Option<LayerId>,
}

fn label_y(row_min_y: f32, row_h: f32, scale: f32) -> f32 {
    let visual_h = (14.0 * scale).max(12.0);
    row_min_y + (row_h - visual_h) * 0.5
}

fn row_selection_id(row: &LayerRow) -> LayerId {
    row.select_as.unwrap_or(row.id)
}

fn row_dimmed_for_workspace(ui_state: &UiState, row: &LayerRow) -> bool {
    if matches!(ui_state.active_workspace, WorkspaceId::AllTools) {
        return false;
    }
    let role = row.domain_role.unwrap_or(match row.role {
        TreeRole::Foundation => DomainRole::ShapeLayer,
        TreeRole::Biome | TreeRole::BiomeSection => DomainRole::TerrainFilter,
        TreeRole::Layer => DomainRole::CompatibilityLegacy,
        TreeRole::ConceptFolder => DomainRole::ShapeLayer,
        _ => DomainRole::CompatibilityLegacy,
    });
    hierarchy_view::emphasis_dims(ui_state.active_workspace, role, row.concept)
}

fn row_hidden_by_filter(
    ui_state: &UiState,
    state: &LayersGuiState,
    row: &LayerRow,
    selected: bool,
) -> bool {
    if selected {
        return false;
    }
    if state.presentation.hide_disabled && !row.enabled {
        return true;
    }
    if state.presentation.show_only_relevant && row_dimmed_for_workspace(ui_state, row) {
        return true;
    }
    false
}

pub fn layer_type_icon(kind: &LayerKind) -> Icon {
    match kind {
        LayerKind::SculptBase(_) | LayerKind::SculptStrokes(_) => Icon::Mountain,
        LayerKind::NoiseValue(_) | LayerKind::NoisePerlin(_) => Icon::Waves,
        LayerKind::HydraulicErosion(_) | LayerKind::ThermalErosion(_) => Icon::Droplets,
        _ => Icon::Layers,
    }
}

fn blank_row(id: LayerId, name: String, depth: u8, role: TreeRole) -> LayerRow {
    LayerRow {
        id,
        name,
        enabled: true,
        type_icon: Icon::Folder,
        depth,
        role,
        domain_role: None,
        concept: None,
        select_as: None,
    }
}

fn collect_rows(doc: &TerrainDocument, state: &LayersGuiState) -> Vec<LayerRow> {
    let mut out = Vec::new();
    let mut terrain = blank_row(
        LayerId::from_u128(TERRAIN_ROOT),
        "Terrain".into(),
        0,
        TreeRole::SectionLabel,
    );
    terrain.type_icon = Icon::Mountain;
    out.push(terrain);

    let order = ArtistConcept::terrain_order();
    let collapsed = &state.presentation.collapsed_groups;

    let mut buckets: Vec<(ArtistConcept, Vec<&StackNode>)> =
        order.iter().map(|c| (*c, Vec::new())).collect();
    for node in &doc.stack.nodes {
        match node {
            StackNode::Group(g)
                if matches!(g.group_kind, terra_core::layer::GroupKind::CategoryFolder)
                    || (matches!(g.group_kind, terra_core::layer::GroupKind::Generic)
                        && g.category.is_some()) =>
            {
                let folder_concept = g
                    .category
                    .map(concept_for_category)
                    .unwrap_or(ArtistConcept::Shape);
                for child in &g.children {
                    let child_concept = match child {
                        StackNode::Group(cg) if cg.is_biome() => ArtistConcept::Biomes,
                        _ => folder_concept,
                    };
                    if let Some(slot) = buckets.iter_mut().find(|(c, _)| *c == child_concept) {
                        slot.1.push(child);
                    }
                }
            }
            _ => {
                let concept = concept_for_top_level(node, false);
                if let Some(slot) = buckets.iter_mut().find(|(c, _)| *c == concept) {
                    slot.1.push(node);
                }
            }
        }
    }

    for (concept, items) in buckets {
        let folder_id = concept_row_id(TERRAIN_SCOPE_KEY, concept);
        let mut folder = blank_row(folder_id, concept.label().into(), 1, TreeRole::ConceptFolder);
        folder.concept = Some(concept);
        folder.type_icon = concept.icon();
        out.push(folder);

        if collapsed.contains(&folder_id) {
            continue;
        }
        for node in items {
            push_node_rows(node, 2, &mut out);
        }
        if concept == ArtistConcept::Masks {
            for mask in &doc.masks {
                let mut row = blank_row(
                    LayerId(mask.id.0),
                    mask.name.clone(),
                    2,
                    TreeRole::MaskAsset,
                );
                row.type_icon = Icon::CircleDot;
                row.domain_role = Some(DomainRole::MaskLayer);
                out.push(row);
            }
        }
    }
    out
}

fn push_node_rows(node: &StackNode, depth: u8, out: &mut Vec<LayerRow>) {
    match node {
        StackNode::Layer(layer) => {
            let role = if matches!(
                layer.kind,
                LayerKind::SculptBase(_) | LayerKind::SculptStrokes(_)
            ) {
                TreeRole::Foundation
            } else {
                TreeRole::Layer
            };
            let mut row = blank_row(layer.id(), layer.common.name.clone(), depth, role);
            row.enabled = layer.common.enabled;
            row.type_icon = layer_type_icon(&layer.kind);
            row.domain_role = Some(DomainRole::ShapeLayer);
            out.push(row);
        }
        StackNode::Group(g) => {
            let role = if g.is_biome() {
                TreeRole::Biome
            } else if g.biome_section_kind().is_some() {
                TreeRole::BiomeSection
            } else {
                TreeRole::Group
            };
            let mut row = blank_row(g.id, g.name.clone(), depth, role);
            row.enabled = g.enabled;
            row.type_icon = if g.is_biome() {
                Icon::Mountain
            } else {
                Icon::Folder
            };
            out.push(row);
            for child in &g.children {
                push_node_rows(child, depth.saturating_add(1), out);
            }
        }
    }
}

pub fn draw_layers_gui(
    ui: &mut GuiContext<'_>,
    doc: &TerrainDocument,
    ui_state: &mut UiState,
    state: &mut LayersGuiState,
) -> Vec<PanelAction> {
    let mut actions = Vec::new();
    let panel = ui.right_layers_rect();
    ui.panel(panel, style::PANEL_BG);

    let header = Rect::from_pos_size(panel.min_x, panel.min_y, panel.width(), style::HEADER_H);
    ui.panel(header, style::PANEL_BG);
    ui.label_at(
        header.min_x + PAD,
        label_y(header.min_y, header.height(), FONT_SCALE * TYPE_LABEL),
        "LAYERS",
        style::TEXT_MUTED,
        FONT_SCALE * TYPE_LABEL,
    );
    let add_hdr = Rect::from_pos_size(
        header.max_x - PAD - 26.0,
        header.min_y + (header.height() - 24.0) * 0.5,
        26.0,
        24.0,
    );
    if icon_button(ui, Id::new("layers_hdr_add"), Icon::Plus, add_hdr) {
        actions.push(PanelAction::OpenQuickAdd);
        state.add_menu_open = false;
    }

    let list = Rect::from_min_max(
        panel.min_x + LIST_INSET,
        header.max_y + 4.0,
        panel.max_x - LIST_INSET,
        panel.max_y - FOOTER_H,
    );
    ui.begin_panel_scrolled(
        Id::new("layers_scroll"),
        list,
        Color::rgba(0.0, 0.0, 0.0, 0.0),
        &mut state.scroll_y,
    );

    let rows = collect_rows(doc, state);
    let mut y = list.min_y + 4.0 - state.scroll_y;
    for row_data in rows {
        let selected = match row_data.role {
            TreeRole::MaskAsset => ui_state.selected_mask == Some(MaskId(row_data.id.0)),
            _ => doc.selected == Some(row_selection_id(&row_data)),
        };
        if row_hidden_by_filter(ui_state, state, &row_data, selected) {
            continue;
        }
        let is_section = matches!(
            row_data.role,
            TreeRole::SectionLabel | TreeRole::ConceptFolder
        );
        let row_h = if is_section { SEC_ROW_H } else { LAYER_ROW_H };
        let row = Rect::from_pos_size(list.min_x, y, list.width(), row_h);
        if selected {
            ui.panel_rounded(row, style::ACCENT_SOFT, 4.0);
        } else if ui.pointer_in(row) {
            ui.panel_rounded(row, style::HOVER_BG, 4.0);
        }

        let indent = row_data.depth as f32 * INDENT_STEP;
        ui.icon_at(
            row.min_x + indent + 4.0,
            row.min_y + (row_h - SEC_ICON_SZ) * 0.5,
            row_data.type_icon,
            if selected {
                style::ACCENT
            } else {
                style::TEXT_MUTED
            },
            SEC_ICON_SZ,
        );
        let label = if row_data.name.is_empty() {
            "(unnamed)"
        } else {
            row_data.name.as_str()
        };
        ui.label_at(
            row.min_x + indent + 24.0,
            label_y(row.min_y, row_h, FONT_SCALE * TYPE_BODY),
            label,
            if selected {
                style::TEXT
            } else {
                style::TEXT_MUTED
            },
            FONT_SCALE * TYPE_BODY,
        );

        let row_id = Id::new(&format!("layer_row_{}", row_data.id.0));
        if ui.pointer_in(row) {
            ui.state.set_hot(row_id);
            if ui.input.primary_pressed {
                ui.state.active = Some(row_id);
            }
        }
        if ui.input.primary_released && ui.state.is_active(row_id) && ui.pointer_in(row) {
            match row_data.role {
                TreeRole::ConceptFolder => {
                    state.presentation.toggle_collapsed(row_data.id);
                    if let Some(concept) = row_data.concept {
                        actions.push(PanelAction::OpenQuickAddConcept { concept });
                    }
                }
                TreeRole::SectionLabel => {}
                TreeRole::MaskAsset => {
                    actions.push(PanelAction::SelectMask(MaskId(row_data.id.0)));
                }
                _ => actions.push(PanelAction::Select(row_selection_id(&row_data))),
            }
        }
        y += row_h;
    }
    ui.end_panel_scrolled(&mut state.scroll_y);

    let footer = Rect::from_pos_size(panel.min_x, panel.max_y - FOOTER_H, panel.width(), FOOTER_H);
    ui.panel(footer, style::PANEL_BG);
    let add = Rect::from_pos_size(
        footer.min_x + PAD,
        footer.min_y + (FOOTER_H - FOOTER_BTN) * 0.5,
        FOOTER_BTN,
        FOOTER_BTN,
    );
    if icon_button(ui, Id::new("layers_add"), Icon::Plus, add) {
        actions.push(PanelAction::OpenQuickAdd);
    }
    actions
}

/// Snapshot for tests / debug: `(id_bits, name, role_tag)`.
pub fn hierarchy_presentation_snapshot(
    doc: &TerrainDocument,
    state: &LayersGuiState,
) -> Vec<(u128, String, &'static str)> {
    collect_rows(doc, state)
        .into_iter()
        .map(|r| {
            let tag = match r.role {
                TreeRole::SectionLabel => "section",
                TreeRole::ConceptFolder => "concept",
                TreeRole::Biome => "biome",
                TreeRole::BiomeSection => "biome_section",
                TreeRole::MaskAsset => "mask",
                TreeRole::Foundation => "foundation",
                TreeRole::Layer => "layer",
                TreeRole::Group => "group",
            };
            (r.id.0.as_u128(), r.name, tag)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_state() -> LayersGuiState {
        LayersGuiState::default()
    }

    #[test]
    fn single_stack_emits_terrain_concepts() {
        let doc = TerrainDocument::new_default();
        let snap = hierarchy_presentation_snapshot(&doc, &default_state());
        assert!(
            snap.iter().any(|r| r.1 == "Terrain" && r.2 == "section"),
            "Terrain root must appear"
        );
        assert!(
            !snap.iter().any(|r| r.2 == "region" || r.2 == "region_mask"),
            "region product rows must not appear"
        );
        let concepts: Vec<_> = snap
            .iter()
            .filter(|r| r.2 == "concept")
            .map(|r| r.1.as_str())
            .collect();
        assert!(
            concepts.contains(&"Shape Layers") || concepts.iter().any(|c| c.contains("Shape")),
            "{concepts:?}"
        );
    }
}
