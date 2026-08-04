use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use terra_core::command::{apply, EditorCommand};
use terra_core::document::EditorSession;
use terra_core::eval::{EvalScheduler, EvalWorkRequest, EvalWorker, PreviewQuality};
use terra_core::heightfield::{Heightfield, HeightfieldMetrics};
use terra_core::layer::{LayerId, LayerKind};
use terra_core::mask::bake_mask_assets;
use terra_gpu::GpuTerrainEngine;
use terra_gui::{GuiContext, GuiInput, GuiRenderer, GuiState, Rect, WidgetLabState};
use terra_io::{load_project, save_project, BackgroundExporter};
use terra_render::{pick_terrain_uv_on_surface, BrushGizmo, TerrainRenderer};
use terra_ui::{
    draw_editor_gui, layers_from_preset, ChromeGuiState, CommandId, DockGuiState,
    InspectorGuiState, LayersGuiState, PanelAction, Preview2dMode, ToolsGuiState, UiState,
    WindowsGuiState,
};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

const EDIT_DEBOUNCE_MS: u128 = 40;
/// While sculpting/painting, rebuild Draft as soon as the previous preview finishes.
const PAINT_DEBOUNCE_MS: u128 = 0;
const REFINE_INTERVAL_MS: u128 = 80;

pub struct TerraApp {
    window: Option<Arc<Window>>,
    renderer: Option<TerrainRenderer>,
    session: EditorSession,
    ui_state: UiState,
    scheduler: EvalScheduler,
    /// Dedicated CPU evaluator for Medium/Full progressive refinement.
    eval_worker: EvalWorker,
    eval_token: u64,
    /// A matching Medium/Full job is queued or executing on the worker.
    worker_refine_pending: bool,
    last_height: Option<Heightfield>,
    project_path: Option<PathBuf>,
    exporter: BackgroundExporter,
    mouse_pressed: Option<MouseButton>,
    last_cursor: Option<(f64, f64)>,
    modifiers_shift: bool,
    modifiers_alt: bool,
    modifiers_ctrl: bool,
    needs_height_upload: bool,
    last_refine: Instant,
    last_edit: Instant,
    pending_eval: bool,
    /// When true, next eval starts from Draft even if already refining.
    force_draft: bool,
    /// Wave C GPU layer preview engine (shares renderer device).
    gpu_engine: Option<GpuTerrainEngine>,
    gui_renderer: Option<GuiRenderer>,
    gui_state: GuiState,
    widget_lab: WidgetLabState,
    chrome_gui: ChromeGuiState,
    tools_gui: ToolsGuiState,
    inspector_gui: InspectorGuiState,
    layers_gui: LayersGuiState,
    windows_gui: WindowsGuiState,
    dock_gui: DockGuiState,
    /// Consumed each frame by terra-gui (mouse wheel).
    gui_scroll_delta: f32,
    /// Text/key edits consumed by the UI search popups on the next redraw.
    gui_text: String,
    gui_backspace: bool,
    gui_escape: bool,
    gui_enter: bool,
    /// Last frame: custom UI captured the pointer (blocks camera/paint).
    gui_wants_pointer: bool,
    /// A sculpt gesture changed the base buffer; represented in History as an annotation.
    sculpt_stroke_active: bool,
    /// App-owned 3D viewport in logical pixels.
    viewport_rect: Rect,
    /// Rebuild live 2D preview only when height/mode changes (not every frame).
    preview_dirty: bool,
    last_preview_mode: Preview2dMode,
}

impl Default for TerraApp {
    fn default() -> Self {
        let now = Instant::now();
        let layout = load_layout_prefs();
        let mut gui_state = GuiState::default();
        gui_state.layout = layout.clone();
        let mut ui_state = UiState::default();
        ui_state.layout = layout;
        Self {
            window: None,
            renderer: None,
            session: EditorSession::new(),
            ui_state,
            scheduler: EvalScheduler::new(),
            eval_worker: EvalWorker::spawn(),
            eval_token: 0,
            worker_refine_pending: false,
            last_height: None,
            project_path: None,
            exporter: BackgroundExporter::new(),
            mouse_pressed: None,
            last_cursor: None,
            modifiers_shift: false,
            modifiers_alt: false,
            modifiers_ctrl: false,
            needs_height_upload: false,
            last_refine: now,
            last_edit: now,
            pending_eval: false,
            force_draft: false,
            gpu_engine: None,
            gui_renderer: None,
            gui_state,
            widget_lab: WidgetLabState::default(),
            chrome_gui: ChromeGuiState::default(),
            tools_gui: ToolsGuiState::default(),
            inspector_gui: InspectorGuiState::default(),
            layers_gui: LayersGuiState::default(),
            windows_gui: WindowsGuiState::default(),
            dock_gui: DockGuiState::default(),
            gui_scroll_delta: 0.0,
            gui_text: String::new(),
            gui_backspace: false,
            gui_escape: false,
            gui_enter: false,
            gui_wants_pointer: false,
            sculpt_stroke_active: false,
            viewport_rect: Rect::from_min_max(88.0, 44.0, 1300.0, 690.0),
            preview_dirty: true,
            last_preview_mode: Preview2dMode::Height,
        }
    }
}

fn layout_prefs_path() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("terra_layout.json")
}

fn load_layout_prefs() -> terra_gui::LayoutPrefs {
    let path = layout_prefs_path();
    if let Ok(bytes) = std::fs::read(&path) {
        if let Ok(prefs) = serde_json::from_slice::<terra_gui::LayoutPrefs>(&bytes) {
            let mut prefs = prefs;
            prefs.clamp_mut();
            return prefs;
        }
    }
    terra_gui::LayoutPrefs::default()
}

fn save_layout_prefs(prefs: &terra_gui::LayoutPrefs) {
    let path = layout_prefs_path();
    if let Ok(json) = serde_json::to_vec_pretty(prefs) {
        let _ = std::fs::write(path, json);
    }
}

impl ApplicationHandler for TerraApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Terra")
                        .with_inner_size(winit::dpi::LogicalSize::new(1600, 900)),
                )
                .expect("window"),
        );
        let renderer = pollster::block_on(TerrainRenderer::new(window.clone())).expect("renderer");

        let gpu_engine = GpuTerrainEngine::new(&renderer.device, 256);
        let gui_renderer =
            GuiRenderer::new(&renderer.device, &renderer.queue, renderer.config.format);
        self.window = Some(window);
        self.renderer = Some(renderer);
        self.gpu_engine = Some(gpu_engine);
        self.gui_renderer = Some(gui_renderer);

        // Initial full dirty + immediate draft so the viewport has content.
        self.mark_all_layers_dirty();
        self.request_rebuild();
        self.pending_eval = false;
        self.run_eval_step();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let mut want_redraw = false;
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(r) = self.renderer.as_mut() {
                    r.resize(size);
                }
                self.refresh_viewport_rect();
                want_redraw = true;
            }
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    if let PhysicalKey::Code(code) = event.physical_key {
                        let bookmark = match code {
                            KeyCode::Digit1 => Some(0usize),
                            KeyCode::Digit2 => Some(1),
                            KeyCode::Digit3 => Some(2),
                            KeyCode::Digit4 => Some(3),
                            KeyCode::Digit5 => Some(4),
                            KeyCode::Digit6 => Some(5),
                            KeyCode::Digit7 => Some(6),
                            KeyCode::Digit8 => Some(7),
                            KeyCode::Digit9 => Some(8),
                            _ => None,
                        };
                        if let Some(index) = bookmark {
                            if self.modifiers_ctrl && !self.modifiers_alt {
                                self.save_camera_bookmark(index);
                            } else if self.modifiers_alt {
                                self.recall_camera_bookmark(index);
                            } else if index < 7 {
                                // Workspace mode shortcuts 1–7 remain available without modifiers.
                                let d = index as u8 + 1;
                                if let Some(mode) = terra_ui::WorkspaceMode::from_digit(d) {
                                    self.ui_state.workspace_mode = mode;
                                    if mode == terra_ui::WorkspaceMode::Sculpt
                                        && !self.ui_state.editor_tool.is_brush()
                                    {
                                        self.ui_state.editor_tool = terra_ui::EditorTool::Move;
                                    }
                                }
                            }
                        }
                        match code {
                            KeyCode::KeyP if self.modifiers_ctrl => {
                                self.dispatch_shortcut(CommandId::OPEN_COMMAND_PALETTE);
                            }
                            KeyCode::KeyL if self.modifiers_ctrl => {
                                self.dispatch_shortcut(CommandId::OPEN_QUICK_ADD);
                            }
                            KeyCode::Insert => self.dispatch_shortcut(CommandId::OPEN_QUICK_ADD),
                            KeyCode::KeyZ if self.modifiers_shift => {
                                self.dispatch_shortcut(CommandId::REDO)
                            }
                            // Keep the original bare-key bindings while supporting standard Ctrl forms.
                            KeyCode::KeyZ => self.dispatch_shortcut(CommandId::UNDO),
                            KeyCode::KeyY => self.dispatch_shortcut(CommandId::REDO),
                            KeyCode::KeyS if self.modifiers_ctrl => {
                                self.dispatch_shortcut(CommandId::SAVE);
                            }
                            KeyCode::Backspace
                                if self.gui_state.wants_text_input()
                                    || self.ui_state.show_quick_add
                                    || self.ui_state.show_command_palette
                                    || !self.ui_state.tool_search.is_empty()
                                    || self.inspector_gui.rename_buffer.is_some() =>
                            {
                                self.gui_backspace = true;
                            }
                            KeyCode::Escape
                                if self.gui_state.wants_text_input()
                                    || self.ui_state.show_quick_add
                                    || self.ui_state.show_command_palette
                                    || self.inspector_gui.rename_buffer.is_some() =>
                            {
                                self.gui_escape = true;
                            }
                            KeyCode::Enter | KeyCode::NumpadEnter
                                if self.gui_state.wants_text_input()
                                    || self.inspector_gui.rename_buffer.is_some() =>
                            {
                                self.gui_enter = true;
                            }
                            _ => {}
                        }
                        let wants_chars = self.gui_state.wants_text_input()
                            || self.ui_state.show_quick_add
                            || self.ui_state.show_command_palette
                            || self.inspector_gui.rename_buffer.is_some()
                            || ui_tool_search_focused(&self.ui_state, &self.gui_state);
                        if !self.modifiers_ctrl && wants_chars {
                            if let Some(ch) = search_character(code, self.modifiers_shift) {
                                self.gui_text.push(ch);
                            }
                        }
                        want_redraw = true;
                    }
                }
            }
            WindowEvent::ModifiersChanged(m) => {
                self.modifiers_shift = m.state().shift_key();
                self.modifiers_alt = m.state().alt_key();
                self.modifiers_ctrl = m.state().control_key();
                self.ui_state.shift_context = self.modifiers_shift;
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let in_viewport = self.cursor_in_viewport();
                let painting = button == MouseButton::Left
                    && self.viewport_paint_active()
                    && in_viewport
                    && !self.modifiers_alt
                    && !self.gui_wants_pointer;
                if state == ElementState::Pressed {
                    self.mouse_pressed = Some(button);
                    if painting {
                        self.paint_at_cursor();
                    }
                } else if state == ElementState::Released {
                    self.mouse_pressed = None;
                    if button == MouseButton::Left && self.sculpt_stroke_active {
                        // Sculpt buffers have no snapshot support yet, so record the artist
                        // gesture honestly without claiming it can restore pixels.
                        self.session.history.push_executed(EditorCommand::Annotate {
                            label: "Sculpted Terrain (not undoable yet)".into(),
                        });
                        self.sculpt_stroke_active = false;
                    }
                }
                want_redraw = true;
            }
            WindowEvent::CursorMoved { position, .. } => {
                let previous = self.last_cursor;
                self.last_cursor = Some((position.x, position.y));
                let painting = self.mouse_pressed == Some(MouseButton::Left)
                    && self.viewport_paint_active()
                    && self.cursor_in_viewport()
                    && !self.modifiers_alt
                    && (!self.gui_wants_pointer || self.sculpt_stroke_active);
                if painting {
                    self.paint_at_cursor();
                    want_redraw = true;
                } else if self.viewport_paint_tool_armed() && self.cursor_in_viewport() {
                    // Keep brush gizmo tracking the cursor.
                    want_redraw = true;
                } else if self.mouse_pressed.is_some() && self.viewport_camera_active() {
                    if let (Some(btn), Some((lx, ly)), Some(r)) =
                        (self.mouse_pressed, previous, self.renderer.as_mut())
                    {
                        let dx = (position.x - lx) as f32;
                        let dy = (position.y - ly) as f32;
                        match btn {
                            MouseButton::Left => r.camera.orbit(dx, dy),
                            MouseButton::Right | MouseButton::Middle => {
                                r.camera.pan(dx, dy);
                                r.camera.clamp_to_world(r.heights.world_size);
                            }
                            _ => {}
                        }
                        want_redraw = true;
                    }
                } else if self.gui_wants_pointer {
                    want_redraw = true;
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let d = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                // Prefer terra-gui scroll over left/right chrome (or any hot widget).
                let over_chrome = self.cursor_logical().is_some_and(|(x, y)| {
                    let vp = &self.viewport_rect;
                    y < vp.min_y || y >= vp.max_y || x < vp.min_x || x >= vp.max_x
                });
                if over_chrome || self.gui_wants_pointer {
                    self.gui_scroll_delta += d;
                    want_redraw = true;
                } else if self.viewport_paint_tool_armed() && !self.modifiers_alt {
                    // Brush tools: wheel adjusts radius; Alt falls through to zoom.
                    self.ui_state.ensure_sculpt_defaults();
                    let step = if d.abs() >= 1.0 {
                        d.signum() * 0.008
                    } else {
                        d * 0.008
                    };
                    self.ui_state.sculpt_radius =
                        (self.ui_state.sculpt_radius + step).clamp(0.005, 0.25);
                    want_redraw = true;
                } else if self.viewport_camera_active() {
                    if let Some(r) = self.renderer.as_mut() {
                        r.camera.zoom(d * 40.0);
                        want_redraw = true;
                    }
                }
            }
            _ => {}
        }

        if want_redraw {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.exporter.poll();
        let mut export_busy = !self.exporter.job.done;
        if export_busy {
            self.ui_state.export_progress = Some(self.exporter.job.progress);
            self.ui_state.status = format!("Export {:.0}%", self.exporter.job.progress * 100.0);
        } else if let Some(result) = self.exporter.job.result.take() {
            self.ui_state.export_progress = None;
            match result {
                Ok(res) => {
                    self.ui_state.status = format!("Exported {}", res.height_path.display());
                }
                Err(err) => {
                    self.ui_state.status = format!("Export failed: {err}");
                }
            }
            export_busy = true; // one more frame to show status
        } else {
            self.ui_state.export_progress = None;
        }

        // Stall heavy eval during camera orbit / UI drag — never during an active
        // sculpt/paint stroke (GUI overlays must not block live height updates).
        let live_paint = self.sculpt_stroke_active
            || (self.mouse_pressed == Some(MouseButton::Left)
                && self.viewport_paint_active()
                && !self.modifiers_alt);
        let stall_draft = if live_paint {
            false
        } else {
            self.mouse_pressed.is_some() || self.gui_wants_pointer
        };
        // Progressive refine waits until the pointer is free (including end of stroke).
        let stall_refine = self.mouse_pressed.is_some() || self.gui_wants_pointer;
        let mut did_eval = false;

        // The worker is never awaited: consume its newest matching result, if available.
        if let Some(result) = self.eval_worker.try_recv_matching(self.eval_token) {
            let quality = result.quality;
            let height = result.height;
            self.scheduler.last_aux = result.aux;
            self.scheduler.last_good = Some(height.clone());
            self.last_height = Some(height);
            self.preview_dirty = true;
            self.needs_height_upload = true;
            self.worker_refine_pending = false;
            self.ui_state.profile.eval_us = result.eval_us;
            self.ui_state.profile.tex_w = self.last_height.as_ref().unwrap().metrics.width;
            self.ui_state.profile.tex_h = self.last_height.as_ref().unwrap().metrics.height;
            self.ui_state.profile.tiles_x = self.last_height.as_ref().unwrap().metrics.tiles_x();
            self.ui_state.profile.tiles_z = self.last_height.as_ref().unwrap().metrics.tiles_z();
            self.ui_state.profile.path = "CPU (async)";
            self.ui_state.profile.quality = match quality {
                PreviewQuality::Draft => "Draft (fast)",
                PreviewQuality::Medium => "Medium",
                PreviewQuality::Full => "Final (viewport)",
                PreviewQuality::Export => "Export quality",
            };
            self.ui_state.quality = quality;
            self.ui_state.build_progress = Some(match quality {
                PreviewQuality::Draft => 1.0 / 3.0,
                PreviewQuality::Medium => 2.0 / 3.0,
                PreviewQuality::Full | PreviewQuality::Export => 1.0,
            });
            self.ui_state.draft_displayed =
                matches!(quality, PreviewQuality::Draft | PreviewQuality::Medium);
            self.ui_state.refining = quality.next_refine().is_some();
            if !self.ui_state.refining {
                self.ui_state.build_progress = None;
                self.ui_state.refining_layer_name = None;
                self.ui_state.draft_displayed = false;
            }
            self.last_refine = Instant::now();
            did_eval = true;
        }

        // Debounced draft eval. During paint/sculpt, key off last eval time — stamps
        // keep resetting last_edit, which would otherwise starve live updates.
        if !stall_draft && self.pending_eval {
            let ready = if live_paint {
                self.last_refine.elapsed().as_millis() >= PAINT_DEBOUNCE_MS
            } else {
                self.last_edit.elapsed().as_millis() >= EDIT_DEBOUNCE_MS
            };
            if ready {
                self.pending_eval = false;
                self.force_draft = true;
                self.run_eval_step();
                self.last_refine = Instant::now();
                did_eval = true;
            }
        }

        // Progressive refine — one quality step per interval, never while interacting.
        if !stall_refine
            && self.ui_state.refining
            && !self.pending_eval
            && !self.worker_refine_pending
            && self.last_refine.elapsed().as_millis() >= REFINE_INTERVAL_MS
        {
            if self.scheduler.advance_quality() {
                self.ui_state.quality = self.scheduler.quality;
                self.enqueue_refine_job();
                did_eval = true;
            } else {
                self.ui_state.refining = false;
                self.ui_state.quality = PreviewQuality::Full;
                self.ui_state.build_progress = None;
                self.ui_state.refining_layer_name = None;
            }
            self.last_refine = Instant::now();
        }

        let work_pending = self.pending_eval
            || self.worker_refine_pending
            || self.ui_state.refining
            || export_busy;
        if live_paint && self.pending_eval {
            // Keep pumping the event loop so Draft can flush between mouse moves.
            event_loop.set_control_flow(ControlFlow::Poll);
        } else if work_pending {
            let wait_ms = if self.pending_eval {
                if live_paint {
                    PAINT_DEBOUNCE_MS
                        .saturating_sub(self.last_refine.elapsed().as_millis())
                        .max(1)
                } else {
                    EDIT_DEBOUNCE_MS
                        .saturating_sub(self.last_edit.elapsed().as_millis())
                        .max(1)
                }
            } else {
                REFINE_INTERVAL_MS
                    .saturating_sub(self.last_refine.elapsed().as_millis())
                    .max(1)
            };
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(wait_ms as u64),
            ));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }

        if did_eval || export_busy || self.needs_height_upload {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
    }
}

impl TerraApp {
    /// Bump generation ID and schedule a debounced rebuild. Does **not** mark all layers dirty.
    fn request_rebuild(&mut self) {
        self.eval_token = self.scheduler.request_rebuild();
        self.eval_worker.set_token(self.eval_token);
        self.worker_refine_pending = false;
        self.ui_state.refining = true;
        self.ui_state.quality = PreviewQuality::Draft;
        self.ui_state.build_progress = Some(0.0);
        self.ui_state.refining_layer_name = self
            .session
            .document
            .selected
            .and_then(|id| self.session.document.stack.find(id))
            .map(|layer| layer.common.name.clone())
            .or_else(|| Some("terrain".into()));
        self.scheduler.quality = PreviewQuality::Draft;
        self.last_edit = Instant::now();
        self.pending_eval = true;
        self.force_draft = true;
        self.ui_state.profile.gen_id = self.eval_token;
    }

    /// Submit the current Medium/Full snapshot to the CPU worker without blocking the UI.
    fn enqueue_refine_job(&mut self) {
        let quality = self.scheduler.quality;
        debug_assert!(matches!(
            quality,
            PreviewQuality::Medium | PreviewQuality::Full
        ));
        self.eval_worker.submit(EvalWorkRequest {
            token: self.eval_token,
            quality,
            stack: self.session.document.stack.clone(),
            masks: self.session.document.masks.clone(),
            base_metrics: self.session.document.metrics,
            preview_res: self.session.document.preview_resolution.min(1024),
            export_res: self.session.document.export_resolution,
            aux: self.scheduler.last_aux.clone(),
            // A worker owns an independent cache, so every new generation begins dirty.
            mark_all_dirty: true,
            dirty_from: None,
        });
        self.worker_refine_pending = true;
        self.ui_state.refining_layer_name = self
            .session
            .document
            .selected
            .and_then(|id| self.session.document.stack.find(id))
            .map(|layer| layer.common.name.clone())
            .or_else(|| Some("terrain".into()));
    }

    fn mark_dirty_from(&mut self, id: LayerId) {
        self.scheduler
            .evaluator
            .mark_dirty_from(&self.session.document.stack, id);
        if let Some(gpu) = self.gpu_engine.as_mut() {
            gpu.mark_dirty_from(&self.session.document.stack, id);
        }
    }

    fn mark_all_layers_dirty(&mut self) {
        self.scheduler
            .evaluator
            .mark_all_dirty(&self.session.document.stack);
        if let Some(gpu) = self.gpu_engine.as_mut() {
            gpu.mark_all_dirty(&self.session.document.stack);
        }
    }

    fn run_eval_step(&mut self) {
        profiling::scope!("eval_step");
        let t0 = Instant::now();
        if self.force_draft {
            self.scheduler.quality = PreviewQuality::Draft;
            self.force_draft = false;
        }
        let preview = self.session.document.preview_resolution.min(1024);
        let export = self.session.document.export_resolution;
        let base = self.session.document.metrics;
        let quality = self.scheduler.quality;
        self.ui_state.refining_layer_name = self
            .session
            .document
            .selected
            .and_then(|id| self.session.document.stack.find(id))
            .map(|layer| layer.common.name.clone())
            .or_else(|| Some("terrain".into()));
        let res = quality.resolution(preview, export);
        let metrics = HeightfieldMetrics {
            width: res,
            height: res,
            world_size_x: base.world_size_x,
            world_size_z: base.world_size_z,
            tile_size: base.tile_size.min(res),
            halo: base.halo,
        };
        let token = self.eval_token;
        // Interactive evaluation must never force a GPU readback/Wait on the UI thread.
        let want_cpu = false;

        let mut used_gpu = false;
        let mut eval_completed = false;
        if token == self.eval_token {
            if let (Some(engine), Some(renderer)) =
                (self.gpu_engine.as_mut(), self.renderer.as_mut())
            {
                match engine.evaluate(
                    &renderer.device,
                    &renderer.queue,
                    &self.session.document.stack,
                    &self.session.document.masks,
                    metrics,
                    quality,
                    want_cpu,
                ) {
                    Ok(result) => {
                        if token != self.eval_token {
                            // Stale generation — discard.
                        } else {
                            let dx = metrics.dx();
                            let dz = metrics.dz();
                            let region = engine.take_dirty_region(1);
                            renderer.present_gpu_height_region(
                                engine.output_texture(),
                                result.width,
                                result.height,
                                result.world_size,
                                result.height_range,
                                dx,
                                dz,
                                region,
                            );
                            self.ui_state.profile.upload_us = renderer.last_upload_us;
                            self.ui_state.profile.clipmap_levels = renderer.last_clipmap_levels;
                            self.ui_state.profile.tex_w = result.width;
                            self.ui_state.profile.tex_h = result.height;
                            self.ui_state.profile.tiles_x = metrics.tiles_x();
                            self.ui_state.profile.tiles_z = metrics.tiles_z();
                            self.ui_state.profile.path = "GPU";
                            self.ui_state.quality = quality;
                            self.ui_state.profile.quality = match quality {
                                PreviewQuality::Draft => "Draft (fast)",
                                PreviewQuality::Medium => "Medium",
                                PreviewQuality::Full => "Final (viewport)",
                                PreviewQuality::Export => "Export quality",
                            };
                            if let Some(hf) = result.cpu {
                                self.scheduler.last_good = Some(hf.clone());
                                self.last_height = Some(hf);
                                self.preview_dirty = true;
                            }
                            used_gpu = true;
                            eval_completed = true;
                        }
                    }
                    Err(_) => {
                        // Unsupported layer / masks → CPU fallback below.
                    }
                }
            }
        }

        if !used_gpu {
            let masks = self.session.document.masks.clone();
            let aux = self.scheduler.last_aux.clone();
            match self.scheduler.run_step(
                &self.session.document.stack,
                base,
                preview,
                export,
                token,
                &masks,
                &aux,
            ) {
                Ok(Some(hf)) => {
                    if token == self.eval_token {
                        let (tw, th) = (hf.metrics.width, hf.metrics.height);
                        self.ui_state.profile.tex_w = tw;
                        self.ui_state.profile.tex_h = th;
                        self.ui_state.profile.path = "CPU";
                        self.ui_state.profile.tiles_x = hf.metrics.tiles_x();
                        self.ui_state.profile.tiles_z = hf.metrics.tiles_z();
                        self.last_height = Some(hf);
                        self.preview_dirty = true;
                        self.needs_height_upload = true;
                        self.ui_state.quality = self.scheduler.quality;
                        self.ui_state.profile.quality = match self.scheduler.quality {
                            PreviewQuality::Draft => "Draft (fast)",
                            PreviewQuality::Medium => "Medium",
                            PreviewQuality::Full => "Final (viewport)",
                            PreviewQuality::Export => "Export quality",
                        };
                        eval_completed = true;
                    }
                }
                Ok(None) => {}
                Err(_) => {}
            }
        }

        let elapsed = t0.elapsed().as_micros() as u64;
        self.ui_state.profile.eval_us = elapsed;
        if eval_completed {
            self.ui_state.quality = quality;
            self.ui_state.build_progress = Some(match quality {
                PreviewQuality::Draft => 1.0 / 3.0,
                PreviewQuality::Medium => 2.0 / 3.0,
                PreviewQuality::Full | PreviewQuality::Export => 1.0,
            });
            self.ui_state.draft_displayed =
                matches!(quality, PreviewQuality::Draft | PreviewQuality::Medium);
            if matches!(quality, PreviewQuality::Full | PreviewQuality::Export) {
                self.ui_state.draft_displayed = false;
            }
        }
    }

    fn refresh_2d_preview(&mut self) {
        if self.ui_state.preview_mode != self.last_preview_mode {
            self.last_preview_mode = self.ui_state.preview_mode;
            self.preview_dirty = true;
        }
        if !self.preview_dirty {
            return;
        }
        let Some(hf) = self.last_height.as_ref() else {
            self.ui_state.preview_rgba = None;
            self.preview_dirty = false;
            return;
        };
        let metrics = hf.metrics;
        let values = match self.ui_state.preview_mode {
            Preview2dMode::Height => {
                let (min, max) = hf.min_max();
                let span = (max - min).max(1e-6);
                hf.to_dense()
                    .into_iter()
                    .map(|value| (value - min) / span)
                    .collect()
            }
            Preview2dMode::Slope => terra_core::analyze::slope_degrees(hf).data().to_vec(),
            Preview2dMode::Flow => {
                let Some(flow) = self.scheduler.last_aux.get("flow_accumulation") else {
                    self.ui_state.preview_rgba = None;
                    return;
                };
                let max = flow
                    .data()
                    .iter()
                    .copied()
                    .into_iter()
                    .fold(1.0f32, f32::max)
                    .ln_1p();
                flow.data()
                    .iter()
                    .copied()
                    .map(|value| value.max(0.0).ln_1p() / max)
                    .collect()
            }
            Preview2dMode::Mask | Preview2dMode::Masks => {
                let baked = bake_mask_assets(
                    &self.session.document.masks,
                    hf,
                    metrics,
                    &self.scheduler.last_aux,
                );
                baked
                    .values()
                    .next()
                    .map(|mask| mask.data().to_vec())
                    .or_else(|| {
                        self.scheduler
                            .last_aux
                            .get("materials")
                            .map(|mask| mask.data().to_vec())
                    })
                    .unwrap_or_else(|| vec![0.0; (metrics.width * metrics.height) as usize])
            }
            Preview2dMode::Material | Preview2dMode::Biome | Preview2dMode::VegetationDensity => {
                let key = match self.ui_state.preview_mode {
                    Preview2dMode::Material => "materials",
                    Preview2dMode::Biome => "biomes",
                    Preview2dMode::VegetationDensity => "vegetation",
                    _ => unreachable!(),
                };
                self.scheduler
                    .last_aux
                    .get(key)
                    .map(|field| field.data().to_vec())
                    .unwrap_or_else(|| vec![0.0; (metrics.width * metrics.height) as usize])
            }
            // TODO: render colorized 2D previews for these diagnostics. The 3D viewport
            // continues to own their shader presentation; retain a height preview here.
            Preview2dMode::Lit
            | Preview2dMode::Unlit
            | Preview2dMode::Curvature
            | Preview2dMode::Convexity
            | Preview2dMode::Concavity
            | Preview2dMode::Normals
            | Preview2dMode::Wireframe
            | Preview2dMode::AmbientOcclusion => {
                let (min, max) = hf.min_max();
                let span = (max - min).max(1e-6);
                hf.to_dense()
                    .into_iter()
                    .map(|value| (value - min) / span)
                    .collect()
            }
        };
        let mut rgba = Vec::with_capacity(values.len() * 4);
        for value in values {
            let value = (value.clamp(0.0, 1.0) * 255.0).round() as u8;
            rgba.extend_from_slice(&[value, value, value, 255]);
        }
        self.ui_state.preview_rgba = Some((metrics.width, metrics.height, rgba));
        self.preview_dirty = false;
    }

    fn save_project_as(&mut self) {
        let default_name = self
            .project_path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("terra_project.json");
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Terra Project", &["json"])
            .set_file_name(default_name)
            .save_file()
        else {
            return;
        };
        match save_project(&self.session.document, &path) {
            Ok(()) => {
                self.ui_state.status = format!("Saved {}", path.display());
                self.project_path = Some(path);
            }
            Err(error) => self.ui_state.status = format!("Save failed: {error}"),
        }
    }

    fn save_current_project(&mut self) {
        if let Some(path) = self.project_path.clone() {
            match save_project(&self.session.document, &path) {
                Ok(()) => self.ui_state.status = format!("Saved {}", path.display()),
                Err(error) => self.ui_state.status = format!("Save failed: {error}"),
            }
        } else {
            self.save_project_as();
        }
    }

    fn load_project_path(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Terra Project", &["json"])
            .pick_file()
        else {
            return;
        };
        match load_project(&path) {
            Ok(doc) => {
                self.session.document = doc;
                self.project_path = Some(path.clone());
                self.mark_all_layers_dirty();
                self.request_rebuild();
                self.run_eval_step();
                self.ui_state.status = format!("Loaded {}", path.display());
            }
            Err(error) => self.ui_state.status = format!("Load failed: {error}"),
        }
    }

    fn choose_export_directory(&mut self) {
        let Some(path) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        self.ui_state.export_path = Some(path.display().to_string());
        self.ui_state.status = format!("Export directory: {}", path.display());
    }

    fn start_export(&mut self) {
        let path = if let Some(existing) = self.ui_state.export_path.clone() {
            std::path::PathBuf::from(existing)
        } else {
            let Some(path) = rfd::FileDialog::new().pick_folder() else {
                return;
            };
            self.ui_state.export_path = Some(path.display().to_string());
            path
        };
        if !self.exporter.job.done {
            self.ui_state.status = "Export already running".into();
            return;
        }
        self.ui_state.export_progress = Some(0.0);
        self.ui_state.status = format!("Exporting to {}", path.display());
        self.exporter.start(self.session.document.clone(), path);
    }

    fn apply_actions(&mut self, actions: Vec<PanelAction>) {
        let mut dirty_from = None;
        let mut sculpt_dirty_rect = None;
        for action in actions {
            match action {
                PanelAction::AddLayer(layer) => {
                    if layer.kind.is_sculpt_base() {
                        // Only one sculpt Base allowed.
                        continue;
                    }
                    let id = layer.id();
                    let index = self.session.document.stack.nodes.len();
                    let cmd = EditorCommand::AddLayer { layer, index };
                    apply(&cmd, &mut self.session.document.stack);
                    self.session.history.push_executed(cmd);
                    self.session.document.selected = Some(id);
                    dirty_from = Some(id);
                }
                PanelAction::RemoveSelected => {
                    if let Some(id) = self.session.document.selected {
                        if self
                            .session
                            .document
                            .stack
                            .find(id)
                            .is_some_and(|l| l.kind.is_sculpt_base())
                        {
                            continue;
                        }
                        if let Some(index) = self.session.document.stack.index_of(id) {
                            if let Some(node) = self.session.document.stack.remove(id) {
                                let cmd = EditorCommand::RemoveLayer { id, node, index };
                                self.session.history.push_executed(cmd);
                                self.session.document.selected =
                                    self.session.document.stack.layer_ids().last().copied();
                                self.mark_all_layers_dirty();
                                self.request_rebuild();
                            }
                        }
                    }
                }
                PanelAction::DuplicateSelected => {
                    if let Some(id) = self.session.document.selected {
                        if self
                            .session
                            .document
                            .stack
                            .find(id)
                            .is_some_and(|l| l.kind.is_sculpt_base())
                        {
                            continue;
                        }
                        if let Some(new_id) = self.session.document.stack.duplicate(id) {
                            let cmd = EditorCommand::Duplicate { source: id, new_id };
                            self.session.history.push_executed(cmd);
                            self.session.document.selected = Some(new_id);
                            dirty_from = Some(new_id);
                        }
                    }
                }
                PanelAction::Reorder { from, to } => {
                    let cmd = EditorCommand::Reorder { from, to };
                    apply(&cmd, &mut self.session.document.stack);
                    self.session.history.push_executed(cmd);
                    self.mark_all_layers_dirty();
                    self.request_rebuild();
                }
                PanelAction::Select(id) => {
                    self.session.document.selected = Some(id);
                }
                PanelAction::SetEnabled { id, enabled } => {
                    let previous = self
                        .session
                        .document
                        .stack
                        .find(id)
                        .map(|l| l.common.enabled)
                        .unwrap_or(true);
                    let cmd = EditorCommand::SetEnabled {
                        id,
                        enabled,
                        previous,
                    };
                    apply(&cmd, &mut self.session.document.stack);
                    self.session.history.push_executed(cmd);
                }
                PanelAction::SetOpacity { id, opacity } => {
                    let previous = self
                        .session
                        .document
                        .stack
                        .find(id)
                        .map(|l| l.common.opacity)
                        .unwrap_or(1.0);
                    let cmd = EditorCommand::SetOpacity {
                        id,
                        opacity,
                        previous,
                    };
                    apply(&cmd, &mut self.session.document.stack);
                    self.session
                        .history
                        .push_coalesced(cmd, Some((coalesce_layer_id(id), "opacity")));
                    dirty_from = Some(id);
                }
                PanelAction::SetBlend { id, blend } => {
                    let previous = self
                        .session
                        .document
                        .stack
                        .find(id)
                        .map(|l| l.common.blend)
                        .unwrap_or_default();
                    let cmd = EditorCommand::SetBlend {
                        id,
                        blend,
                        previous,
                    };
                    apply(&cmd, &mut self.session.document.stack);
                    self.session.history.push_executed(cmd);
                    dirty_from = Some(id);
                }
                PanelAction::SetKind { id, kind } => {
                    let previous = self
                        .session
                        .document
                        .stack
                        .find(id)
                        .map(|l| l.kind.clone())
                        .unwrap_or(LayerKind::Flat(Default::default()));
                    let cmd = EditorCommand::SetKind { id, kind, previous };
                    apply(&cmd, &mut self.session.document.stack);
                    self.session
                        .history
                        .push_coalesced(cmd, Some((coalesce_layer_id(id), "kind")));
                    dirty_from = Some(id);
                }
                PanelAction::Rename { id, name } => {
                    let previous = self
                        .session
                        .document
                        .stack
                        .find(id)
                        .map(|l| l.common.name.clone())
                        .unwrap_or_default();
                    let cmd = EditorCommand::Rename { id, name, previous };
                    apply(&cmd, &mut self.session.document.stack);
                    self.session.history.push_executed(cmd);
                }
                PanelAction::ApplyPreset(name) => {
                    if let Some(layers) = layers_from_preset(&name) {
                        self.session.document.stack.nodes.clear();
                        for layer in layers {
                            self.session.document.stack.push(layer);
                        }
                        self.session.document.selected = self
                            .session
                            .document
                            .stack
                            .flatten_layers()
                            .iter()
                            .find(|l| l.kind.is_sculpt_base())
                            .map(|l| l.id())
                            .or_else(|| self.session.document.stack.layer_ids().first().copied());
                        self.session.document.presets_used.push(name);
                        self.ui_state.editor_tool = terra_ui::EditorTool::Raise;
                        self.ui_state.paint_mask = None;
                        self.mark_all_layers_dirty();
                        self.request_rebuild();
                    }
                }
                PanelAction::AddMask(asset) => {
                    self.session.document.masks.push(asset);
                    dirty_from = self.session.document.selected;
                }
                PanelAction::UpdateMaskAsset(asset) => {
                    if let Some(existing) = self
                        .session
                        .document
                        .masks
                        .iter_mut()
                        .find(|existing| existing.id == asset.id)
                    {
                        *existing = asset;
                        dirty_from = self.session.document.selected;
                    }
                }
                PanelAction::BindMaskToLayer { layer, mask } => {
                    if let Some(target) = self.session.document.stack.find_mut(layer) {
                        if !target.common.masks.iter().any(|binding| binding.id == mask) {
                            target
                                .common
                                .masks
                                .push(terra_core::mask::MaskRef::new(mask));
                            dirty_from = Some(layer);
                        }
                    }
                }
                PanelAction::UnbindMask { layer, mask } => {
                    if let Some(target) = self.session.document.stack.find_mut(layer) {
                        target.common.masks.retain(|binding| binding.id != mask);
                        dirty_from = Some(layer);
                    }
                }
                PanelAction::UpdateMaskBinding {
                    layer,
                    mask,
                    strength,
                    invert,
                } => {
                    if let Some(binding) =
                        self.session
                            .document
                            .stack
                            .find_mut(layer)
                            .and_then(|target| {
                                target
                                    .common
                                    .masks
                                    .iter_mut()
                                    .find(|binding| binding.id == mask)
                            })
                    {
                        binding.strength = strength;
                        binding.invert = invert;
                        dirty_from = Some(layer);
                    }
                }
                PanelAction::PaintMaskStamp {
                    mask_id,
                    u,
                    v,
                    radius,
                    strength,
                    erase,
                } => {
                    if let Some(asset) = self
                        .session
                        .document
                        .masks
                        .iter_mut()
                        .find(|asset| asset.id == mask_id)
                    {
                        let paint = asset
                            .paint
                            .get_or_insert_with(|| terra_core::mask::PaintBuffer::new(512, 512));
                        paint.stamp_circle(u, v, radius, strength, erase);
                        dirty_from = self.session.document.selected;
                    }
                }
                PanelAction::PaintSculptStamp {
                    layer,
                    u,
                    v,
                    radius,
                    strength,
                    mode,
                } => {
                    if let Some(target) = self.session.document.stack.find_mut(layer) {
                        if let terra_core::layer::LayerKind::SculptBase(params) = &mut target.kind {
                            params.stamp_circle(u, v, radius, strength, mode);
                            // Dirty from Base so Hills / Weather re-evaluate on new foundation.
                            dirty_from = Some(layer);
                            let resolution = self.scheduler.quality.resolution(
                                self.session.document.preview_resolution.min(1024),
                                self.session.document.export_resolution,
                            );
                            let x0 =
                                ((u - radius).clamp(0.0, 1.0) * resolution as f32).floor() as u32;
                            let y0 =
                                ((v - radius).clamp(0.0, 1.0) * resolution as f32).floor() as u32;
                            let x1 =
                                ((u + radius).clamp(0.0, 1.0) * resolution as f32).ceil() as u32;
                            let y1 =
                                ((v + radius).clamp(0.0, 1.0) * resolution as f32).ceil() as u32;
                            sculpt_dirty_rect = Some((
                                x0,
                                y0,
                                x1.saturating_sub(x0).max(1),
                                y1.saturating_sub(y0).max(1),
                            ));
                            self.session.document.selected = Some(layer);
                        }
                    }
                }
                PanelAction::ResetSculptBase { id } => {
                    if let Some(target) = self.session.document.stack.find_mut(id) {
                        if let terra_core::layer::LayerKind::SculptBase(params) = &mut target.kind {
                            params.reset();
                            dirty_from = Some(id);
                        }
                    }
                }
                PanelAction::MarkDirty(id) => {
                    dirty_from = id.or(self.session.document.selected);
                }
                PanelAction::SetLocked { id, locked } => {
                    let previous = self
                        .session
                        .document
                        .stack
                        .find(id)
                        .map(|l| l.common.locked)
                        .unwrap_or(false);
                    let cmd = EditorCommand::SetLocked {
                        id,
                        locked,
                        previous,
                    };
                    apply(&cmd, &mut self.session.document.stack);
                    self.session.history.push_executed(cmd);
                }
                PanelAction::SetSolo { id, solo } => {
                    let previous = self
                        .session
                        .document
                        .stack
                        .find(id)
                        .map(|l| l.common.solo)
                        .unwrap_or(false);
                    let cmd = EditorCommand::SetSolo { id, solo, previous };
                    apply(&cmd, &mut self.session.document.stack);
                    self.session.history.push_executed(cmd);
                    dirty_from = Some(id);
                }
                PanelAction::SetColorTag { id, tag } => {
                    let previous = self
                        .session
                        .document
                        .stack
                        .find(id)
                        .map(|l| l.common.color_tag)
                        .unwrap_or(0);
                    let cmd = EditorCommand::SetColorTag { id, tag, previous };
                    apply(&cmd, &mut self.session.document.stack);
                    self.session.history.push_executed(cmd);
                }
                PanelAction::SetCached { id, cached } => {
                    let previous = self
                        .session
                        .document
                        .stack
                        .find(id)
                        .map(|l| l.common.cached)
                        .unwrap_or(false);
                    let cmd = EditorCommand::SetCached {
                        id,
                        cached,
                        previous,
                    };
                    apply(&cmd, &mut self.session.document.stack);
                    self.session.history.push_executed(cmd);
                    dirty_from = Some(id);
                }
                PanelAction::AddGroup { name } => {
                    let id = terra_core::layer::LayerId::new();
                    let index = self.session.document.stack.nodes.len();
                    let cmd = EditorCommand::AddGroup { name, id, index };
                    apply(&cmd, &mut self.session.document.stack);
                    self.session.history.push_executed(cmd);
                    self.session.document.selected = Some(id);
                }
                PanelAction::OpenQuickAdd => {
                    self.ui_state.show_quick_add = true;
                }
                PanelAction::RandomizeSeed { id } => {
                    if let Some(layer) = self.session.document.stack.find_mut(id) {
                        randomize_layer_seed(&mut layer.kind);
                        dirty_from = Some(id);
                    }
                }
            }
        }
        let sculpt_stamp = sculpt_dirty_rect.is_some();
        if let Some(rect) = sculpt_dirty_rect {
            if let Some(gpu) = self.gpu_engine.as_mut() {
                gpu.set_dirty_rect(Some(rect));
            }
        }
        if let Some(id) = dirty_from {
            // Suffix-only dirty — do not mark_all_dirty (preserves layer cache).
            // Sculpt stamps only change the base paint buffer; keep GPU dependents clean so
            // Draft can reuse cached noise/shape contributions and just re-blend.
            let sculpt_only = sculpt_stamp
                && matches!(
                    self.session
                        .document
                        .stack
                        .find(id)
                        .map(|layer| &layer.kind),
                    Some(terra_core::layer::LayerKind::SculptBase(_))
                );
            self.scheduler
                .evaluator
                .mark_dirty_from(&self.session.document.stack, id);
            if let Some(gpu) = self.gpu_engine.as_mut() {
                if sculpt_only {
                    gpu.mark_dirty(id);
                } else {
                    gpu.mark_dirty_from(&self.session.document.stack, id);
                }
            }
            self.request_rebuild();
        }
    }

    fn cursor_logical(&self) -> Option<(f32, f32)> {
        let (cursor, window) = (self.last_cursor?, self.window.as_ref()?);
        let ppp = window.scale_factor() as f32;
        Some((cursor.0 as f32 / ppp, cursor.1 as f32 / ppp))
    }

    fn cursor_in_viewport(&self) -> bool {
        let Some((x, y)) = self.cursor_logical() else {
            return false;
        };
        self.viewport_rect.contains(x, y)
    }

    /// Camera owns the pointer over the 3D viewport (and not over terra-gui).
    /// Move tool (and non-brush tools) navigate; Alt temporarily restores camera while brushing.
    fn viewport_camera_active(&self) -> bool {
        if self.gui_wants_pointer || !self.cursor_in_viewport() {
            return false;
        }
        self.modifiers_alt || !self.viewport_paint_active()
    }

    /// True when left-drag should stamp Base heights or a mask.
    fn viewport_paint_active(&self) -> bool {
        if self.ui_state.editor_tool.is_sculpt() {
            return self
                .session
                .document
                .stack
                .flatten_layers()
                .iter()
                .any(|l| l.kind.is_sculpt_base());
        }
        self.ui_state.editor_tool == terra_ui::EditorTool::PaintMask
            && self.ui_state.paint_mask.is_some()
    }

    fn refresh_viewport_rect(&mut self) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let ppp = window.scale_factor() as f32;
        let size = window.inner_size();
        let screen_w = size.width as f32 / ppp;
        let screen_h = size.height as f32 / ppp;
        self.viewport_rect =
            GuiContext::viewport_rect_for(screen_w, screen_h, &self.ui_state.layout);
    }

    fn paint_at_cursor(&mut self) {
        let Some((u, v)) = self.pick_paint_uv() else {
            return;
        };

        if let Some(mode) = self.ui_state.editor_tool.sculpt_mode() {
            let layer_id = self
                .session
                .document
                .stack
                .flatten_layers()
                .iter()
                .find(|l| l.kind.is_sculpt_base())
                .map(|l| l.id());
            let Some(layer_id) = layer_id else {
                return;
            };
            self.ui_state.ensure_sculpt_defaults();
            self.sculpt_stroke_active = true;
            let strength = if mode == 2 {
                (self.ui_state.sculpt_strength / 10.0).clamp(0.05, 1.0)
            } else {
                self.ui_state.sculpt_strength
            };
            self.apply_actions(vec![PanelAction::PaintSculptStamp {
                layer: layer_id,
                u,
                v,
                radius: self.ui_state.sculpt_radius,
                strength,
                mode,
            }]);
            self.flush_live_paint_preview();
            return;
        }

        let Some(mask_id) = self.ui_state.paint_mask else {
            return;
        };
        self.apply_actions(vec![PanelAction::PaintMaskStamp {
            mask_id,
            u,
            v,
            radius: self.ui_state.sculpt_radius.max(0.02),
            strength: 0.18,
            erase: false,
        }]);
        self.flush_live_paint_preview();
    }

    /// Push Draft heights to the GPU immediately while a brush stroke is active.
    fn flush_live_paint_preview(&mut self) {
        if !self.pending_eval {
            return;
        }
        if self.last_refine.elapsed().as_millis() < PAINT_DEBOUNCE_MS {
            return;
        }
        self.pending_eval = false;
        self.force_draft = true;
        self.run_eval_step();
        self.last_refine = Instant::now();
    }

    /// Raycast cursor onto the height surface → terrain UV (same mapping as the brush gizmo).
    fn pick_paint_uv(&self) -> Option<(f32, f32)> {
        let (x, y) = self.cursor_logical()?;
        if !self.viewport_rect.contains(x, y) {
            return None;
        }
        // Once a stroke has started, keep stamping even if the cursor grazes overlays
        // (brush bar / gizmo chrome) — otherwise live preview stalls mid-drag.
        if self.gui_wants_pointer && !self.sculpt_stroke_active {
            return None;
        }
        let renderer = self.renderer.as_ref()?;
        let window = self.window.as_ref()?;
        let ppp = window.scale_factor() as f32;
        let screen_w = renderer.config.width as f32 / ppp;
        let screen_h = renderer.config.height as f32 / ppp;
        let aspect = renderer.config.width as f32 / renderer.config.height.max(1) as f32;
        pick_terrain_uv_on_surface(
            &renderer.camera,
            aspect,
            (x, y),
            (screen_w, screen_h),
            renderer.heights.world_size,
            self.last_height.as_ref(),
        )
    }

    fn brush_gizmo_color(&self) -> [f32; 4] {
        use terra_ui::EditorTool::*;
        match self.ui_state.editor_tool {
            Raise => [0.25, 0.75, 1.0, 0.95],
            Lower => [1.0, 0.45, 0.2, 0.95],
            Smooth => [1.0, 0.9, 0.35, 0.95],
            PaintMask => [0.95, 0.95, 1.0, 0.9],
            _ => [0.5, 0.8, 1.0, 0.8],
        }
    }

    fn update_brush_gizmo(&mut self) {
        let show = self.viewport_paint_tool_armed()
            && self.cursor_in_viewport()
            && !self.gui_wants_pointer
            && !self.modifiers_alt;
        if !show {
            if let Some(renderer) = self.renderer.as_mut() {
                renderer.set_brush_gizmo(None);
                renderer.sync_brush_geometry(None);
            }
            return;
        }
        self.ui_state.ensure_sculpt_defaults();
        let radius = if self.ui_state.editor_tool.is_sculpt() {
            self.ui_state.sculpt_radius
        } else {
            self.ui_state.sculpt_radius.max(0.02)
        };
        let color = self.brush_gizmo_color();
        let Some((u, v)) = self.pick_paint_uv() else {
            if let Some(renderer) = self.renderer.as_mut() {
                renderer.set_brush_gizmo(None);
                renderer.sync_brush_geometry(None);
            }
            return;
        };
        let world = self
            .renderer
            .as_ref()
            .map(|r| r.heights.world_size)
            .unwrap_or((4096.0, 4096.0));
        let ring_y = terra_render::BrushOverlay::sample_ring_heights(
            self.last_height.as_ref(),
            world,
            u,
            v,
            radius,
        );
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.set_brush_gizmo(Some(BrushGizmo {
                u,
                v,
                radius_uv: radius,
                color,
            }));
            renderer.sync_brush_geometry(Some(&ring_y));
        }
    }

    /// True when a paint/sculpt tool is selected (gizmo should track the cursor).
    fn viewport_paint_tool_armed(&self) -> bool {
        self.ui_state.editor_tool.is_sculpt()
            || (self.ui_state.editor_tool == terra_ui::EditorTool::PaintMask
                && self.ui_state.paint_mask.is_some())
    }

    /// Execute keyboard bindings through the shared command IDs.
    fn dispatch_shortcut(&mut self, command: &str) {
        match command {
            CommandId::OPEN_COMMAND_PALETTE => self.ui_state.show_command_palette = true,
            CommandId::OPEN_QUICK_ADD => self.ui_state.show_quick_add = true,
            CommandId::UNDO => self.undo(),
            CommandId::REDO => self.redo(),
            CommandId::SAVE => self.save_current_project(),
            _ => {}
        }
    }

    fn save_camera_bookmark(&mut self, index: usize) {
        let Some(renderer) = self.renderer.as_ref() else {
            return;
        };
        let camera = &renderer.camera;
        self.ui_state.bookmarks[index] = Some(terra_ui::CameraBookmark {
            x: camera.target.x,
            y: camera.target.y,
            z: camera.target.z,
            yaw: camera.yaw,
            pitch: camera.pitch,
            distance: camera.distance,
        });
        self.ui_state.status = format!("Saved camera bookmark {}", index + 1);
    }

    fn recall_camera_bookmark(&mut self, index: usize) {
        let Some(bookmark) = self.ui_state.bookmarks[index] else {
            self.ui_state.status = format!("Camera bookmark {} is empty", index + 1);
            return;
        };
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.camera.target.x = bookmark.x;
            renderer.camera.target.y = bookmark.y;
            renderer.camera.target.z = bookmark.z;
            renderer.camera.yaw = bookmark.yaw;
            renderer.camera.pitch = bookmark.pitch;
            renderer.camera.distance = bookmark.distance;
            renderer.camera.clamp_to_world(renderer.heights.world_size);
            self.ui_state.status = format!("Recalled camera bookmark {}", index + 1);
        }
    }

    fn undo(&mut self) {
        if let Some(id) = self.session.history.undo(&mut self.session.document.stack) {
            self.mark_dirty_from(id);
        } else {
            self.mark_all_layers_dirty();
        }
        self.request_rebuild();
    }

    fn redo(&mut self) {
        if let Some(id) = self.session.history.redo(&mut self.session.document.stack) {
            self.mark_dirty_from(id);
        } else {
            self.mark_all_layers_dirty();
        }
        self.request_rebuild();
    }

    fn redraw(&mut self) {
        let frame_t0 = Instant::now();
        let Some(window) = self.window.clone() else {
            return;
        };

        self.refresh_viewport_rect();
        self.refresh_2d_preview();

        if self.needs_height_upload {
            if let Some(hf) = self.last_height.clone() {
                if let Some(r) = self.renderer.as_mut() {
                    r.upload_heightfield(&hf);
                    r.upload_aux_maps(
                        self.scheduler.last_aux.get("materials"),
                        self.scheduler.last_aux.get("wetness"),
                        self.scheduler.last_aux.get("vegetation"),
                    );
                    self.ui_state.profile.upload_us = r.last_upload_us;
                }
                self.needs_height_upload = false;
            }
        }

        let pointer = self.cursor_logical();
        let primary_down = self.mouse_pressed == Some(MouseButton::Left);
        let scroll_delta = std::mem::take(&mut self.gui_scroll_delta);
        let text = std::mem::take(&mut self.gui_text);
        let backspace_pressed = std::mem::take(&mut self.gui_backspace);
        let escape_pressed = std::mem::take(&mut self.gui_escape);
        let enter_pressed = std::mem::take(&mut self.gui_enter);
        let pixels_per_point = window.scale_factor() as f32;

        // Brush ring tracks the cursor on the height surface while sculpt/mask tools are armed.
        self.update_brush_gizmo();

        let ui_out = {
            let Some(renderer) = self.renderer.as_mut() else {
                return;
            };
            let Some(gui_renderer) = self.gui_renderer.as_mut() else {
                return;
            };

            // Terrain pass — always draws last-good GPU textures (never waits on eval).
            let render_t0 = Instant::now();
            {
                let (light_dir, exposure, clear) = self.ui_state.lighting_preset.params();
                renderer.lighting.light_dir = light_dir;
                renderer.lighting.exposure = exposure;
                renderer.lighting.clear = clear;
            }
            let frame = match renderer.render_terrain() {
                Ok(f) => f,
                Err(e) => {
                    log::warn!("render: {e}");
                    return;
                }
            };
            self.ui_state.profile.render_us = render_t0.elapsed().as_micros() as u64;
            self.ui_state.profile.clipmap_levels = renderer.last_clipmap_levels;
            self.ui_state.camera_xz = (
                (renderer.camera.target.x / renderer.heights.world_size.0.max(1.0)).clamp(0.0, 1.0),
                (renderer.camera.target.z / renderer.heights.world_size.1.max(1.0)).clamp(0.0, 1.0),
            );
            self.ui_state.camera_yaw = renderer.camera.yaw;
            let view = frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());

            let screen_w = renderer.config.width as f32 / pixels_per_point;
            let screen_h = renderer.config.height as f32 / pixels_per_point;

            let ui_t0 = Instant::now();
            self.ui_state.history_descriptions = self.session.history.undo_descriptions();
            self.gui_state.layout = self.ui_state.layout.clone();
            let mut gui = GuiContext::begin(
                screen_w,
                screen_h,
                pixels_per_point,
                GuiInput {
                    pointer,
                    primary_down,
                    scroll_delta,
                    text,
                    backspace_pressed,
                    escape_pressed,
                    enter_pressed,
                    ..Default::default()
                },
                &mut self.gui_state,
            );
            self.viewport_rect = gui.viewport_rect();

            let ui_out = draw_editor_gui(
                &mut gui,
                &mut self.session.document,
                &mut self.ui_state,
                &mut self.chrome_gui,
                &mut self.tools_gui,
                &mut self.layers_gui,
                &mut self.inspector_gui,
                &mut self.windows_gui,
                &mut self.dock_gui,
            );
            if self.ui_state.show_widget_lab {
                gui.widget_lab(&mut self.widget_lab);
            }
            gui.end();
            self.gui_wants_pointer = gui.wants_pointer() || self.ui_state.tool_drag.is_some();
            self.ui_state.profile.ui_us = ui_t0.elapsed().as_micros() as u64;

            gui_renderer.render(
                &renderer.device,
                &renderer.queue,
                &view,
                &mut gui,
                renderer.config.width,
                renderer.config.height,
            );
            frame.present();
            ui_out
        };

        self.ui_state.profile.frame_us = frame_t0.elapsed().as_micros() as u64;

        self.apply_actions(ui_out.actions);
        if ui_out.request_undo {
            self.undo();
        }
        if ui_out.request_redo {
            self.redo();
        }
        if ui_out.request_save_as {
            self.save_project_as();
        }
        if ui_out.request_load_path {
            self.load_project_path();
        }
        if ui_out.request_export_path {
            self.choose_export_directory();
        }
        if ui_out.request_start_export {
            self.start_export();
        }
        if ui_out.camera_reset {
            if let Some(r) = self.renderer.as_mut() {
                r.request_camera_reframe();
                r.frame_camera_to_terrain();
            }
        }
        if ui_out.camera_top_view {
            if let Some(r) = self.renderer.as_mut() {
                r.camera_top_view();
            }
        }
        if ui_out.camera_frame_selection {
            if let Some(r) = self.renderer.as_mut() {
                r.frame_camera_to_selection();
            }
        }
        if let Some((u, v)) = ui_out.request_camera_focus_uv {
            if let Some(r) = self.renderer.as_mut() {
                r.focus_camera_uv(u, v);
            }
        }
        if ui_out.request_cancel_build {
            // Bump the eval token so the worker discards the in-flight job.
            self.eval_token = self.eval_token.wrapping_add(1);
            self.eval_worker.set_token(self.eval_token);
            self.ui_state.refining = false;
            self.ui_state.build_progress = None;
            self.ui_state.refining_layer_name = None;
            self.ui_state.status = "Build cancelled".into();
        }
        if let Some(res) = self.ui_state.pending_preview_resolution.take() {
            if self.session.document.preview_resolution != res {
                self.session.document.preview_resolution = res;
                self.request_rebuild();
            }
        }
        if ui_out.request_save_bookmark {
            let slot = self
                .ui_state
                .bookmarks
                .iter()
                .position(|b| b.is_none())
                .unwrap_or(0);
            self.save_camera_bookmark(slot);
        }
        if let Some(slot) = ui_out.request_save_bookmark_slot {
            self.save_camera_bookmark(slot);
        }
        if let Some(slot) = ui_out.request_recall_bookmark {
            self.recall_camera_bookmark(slot);
        }
        if self.ui_state.layout_dirty {
            self.ui_state.layout.clamp_mut();
            save_layout_prefs(&self.ui_state.layout);
            self.ui_state.layout_dirty = false;
            self.refresh_viewport_rect();
        }

        if self.gui_wants_pointer {
            window.request_redraw();
        }
    }
}

fn ui_tool_search_focused(ui_state: &UiState, gui_state: &GuiState) -> bool {
    // Tool search uses hot/active on its field id, or any text focus.
    gui_state.wants_text_input()
        || gui_state.hot == Some(terra_gui::Id::new("tool_search"))
        || gui_state.active == Some(terra_gui::Id::new("tool_search"))
        || !ui_state.tool_search.is_empty()
}

/// Physical-key fallback for the custom GUI's search fields.
fn search_character(code: KeyCode, shift: bool) -> Option<char> {
    let ch = match code {
        KeyCode::KeyA => 'a',
        KeyCode::KeyB => 'b',
        KeyCode::KeyC => 'c',
        KeyCode::KeyD => 'd',
        KeyCode::KeyE => 'e',
        KeyCode::KeyF => 'f',
        KeyCode::KeyG => 'g',
        KeyCode::KeyH => 'h',
        KeyCode::KeyI => 'i',
        KeyCode::KeyJ => 'j',
        KeyCode::KeyK => 'k',
        KeyCode::KeyL => 'l',
        KeyCode::KeyM => 'm',
        KeyCode::KeyN => 'n',
        KeyCode::KeyO => 'o',
        KeyCode::KeyP => 'p',
        KeyCode::KeyQ => 'q',
        KeyCode::KeyR => 'r',
        KeyCode::KeyS => 's',
        KeyCode::KeyT => 't',
        KeyCode::KeyU => 'u',
        KeyCode::KeyV => 'v',
        KeyCode::KeyW => 'w',
        KeyCode::KeyX => 'x',
        KeyCode::KeyY => 'y',
        KeyCode::KeyZ => 'z',
        KeyCode::Space => ' ',
        _ => return None,
    };
    Some(if shift { ch.to_ascii_uppercase() } else { ch })
}

fn randomize_layer_seed(kind: &mut terra_core::layer::LayerKind) {
    use terra_core::layer::LayerKind;
    let seed = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(1))
        ^ 0xA5A5_1234;
    match kind {
        LayerKind::NoiseValue(p) | LayerKind::NoisePerlin(p) | LayerKind::NoiseOpenSimplex(p) => {
            p.seed = seed
        }
        LayerKind::NoiseWorley(p) => p.base.seed = seed,
        LayerKind::Fbm(p) | LayerKind::Ridged(p) => p.base.seed = seed,
        LayerKind::DomainWarp(p) => p.base.seed = seed,
        LayerKind::Mountains(p) => p.base.seed = seed,
        LayerKind::Dunes(p) => p.base.seed = seed,
        LayerKind::Canyons(p) => p.seed = seed,
        LayerKind::VoronoiRegions(p) => p.base.seed = seed,
        LayerKind::Vegetation(p) => p.seed = seed,
        _ => {}
    }
}

/// Stable compact key for associating consecutive control updates with a layer.
fn coalesce_layer_id(id: LayerId) -> u64 {
    let raw = id.0.as_u128();
    raw as u64 ^ (raw >> 64) as u64
}
