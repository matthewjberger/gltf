use nightshade::ecs::camera::commands::spawn_pan_orbit_camera;
use nightshade::ecs::camera::systems::pan_orbit_camera_system;
use nightshade::ecs::graphics::resources::PbrDebugMode;
use nightshade::ecs::prefab::resources::mesh_cache_insert;
use nightshade::prelude::*;
use nightshade::render::wgpu::passes;
use nightshade::render::wgpu::rendergraph::RenderGraph;
use nightshade::run::RenderResources;
use std::path::PathBuf;

const DEFAULT_HDR_BYTES: &[u8] = include_bytes!("../assets/sky/moonrise.hdr");
const DEFAULT_GLTF_BYTES: &[u8] = include_bytes!("../assets/gltf/DamagedHelmet.glb");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    launch(ViewerState::default())
}

struct CustomSkybox {
    name: String,
    path: PathBuf,
}

struct ViewerState {
    model_entities: Vec<Entity>,
    camera_entity: Option<Entity>,
    rotation_speed: f32,
    loaded: bool,
    left_arrow_was_pressed: bool,
    right_arrow_was_pressed: bool,
    previous_atmosphere: Atmosphere,
    custom_skyboxes: Vec<CustomSkybox>,
    selected_custom_skybox: Option<usize>,
    drag_file_type: Option<String>,
    sun_entity: Option<Entity>,
}

impl Default for ViewerState {
    fn default() -> Self {
        Self {
            model_entities: Vec::new(),
            camera_entity: None,
            rotation_speed: 0.0,
            loaded: false,
            left_arrow_was_pressed: false,
            right_arrow_was_pressed: false,
            previous_atmosphere: Atmosphere::Hdr,
            custom_skyboxes: Vec::new(),
            selected_custom_skybox: None,
            drag_file_type: None,
            sun_entity: None,
        }
    }
}

impl State for ViewerState {
    fn title(&self) -> &str {
        "glTF Viewer"
    }

    fn configure_render_graph(
        &mut self,
        graph: &mut RenderGraph<World>,
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        resources: RenderResources,
    ) {
        let (width, height) = (1920, 1080);
        let bloom_width = width / 2;
        let bloom_height = height / 2;

        let bloom_texture = graph
            .add_color_texture("bloom")
            .format(wgpu::TextureFormat::Rgba16Float)
            .size(bloom_width, bloom_height)
            .clear_color(wgpu::Color::BLACK)
            .transient();

        let bloom_pass = passes::BloomPass::new(device, width, height);
        graph
            .pass(Box::new(bloom_pass))
            .read("hdr", resources.scene_color)
            .write("bloom", bloom_texture);

        let ssao_pass = passes::SsaoPass::new(device);
        graph
            .pass(Box::new(ssao_pass))
            .read("depth", resources.depth)
            .read("view_normals", resources.view_normals)
            .write("ssao_raw", resources.ssao_raw);

        let ssao_blur_pass = passes::SsaoBlurPass::new(device);
        graph
            .pass(Box::new(ssao_blur_pass))
            .read("ssao_raw", resources.ssao_raw)
            .write("ssao", resources.ssao);

        let postprocess_pass = passes::PostProcessPass::new(device, surface_format, 0.08);
        graph
            .pass(Box::new(postprocess_pass))
            .read("hdr", resources.scene_color)
            .read("bloom", bloom_texture)
            .read("ssao", resources.ssao)
            .write("output", resources.swapchain);
    }

    fn initialize(&mut self, world: &mut World) {
        world.resources.user_interface.enabled = true;
        world.resources.graphics.show_grid = false;
        world.resources.graphics.atmosphere = Atmosphere::Hdr;
        world.resources.graphics.use_fullscreen = false;
        world.resources.graphics.bloom_intensity = 0.08;
        world.resources.graphics.ssao_enabled = true;
        world.resources.graphics.ssao_radius = 0.5;
        world.resources.graphics.ssao_bias = 0.025;
        world.resources.graphics.ssao_intensity = 1.5;

        load_hdr_skybox(world, DEFAULT_HDR_BYTES.to_vec());

        let sun = spawn_sun(world);
        if let Some(light) = world.get_light_mut(sun) {
            light.cast_shadows = true;
        }
        self.sun_entity = Some(sun);

        self.rotation_speed = 0.0;

        let camera_entity = spawn_pan_orbit_camera(
            world,
            Vec3::new(0.0, 0.0, 0.0),
            5.0,
            0.0,
            0.3,
            "Main Camera".to_string(),
        );

        self.camera_entity = Some(camera_entity);
        world.resources.active_camera = Some(camera_entity);

        self.load_gltf_from_bytes(world, DEFAULT_GLTF_BYTES);
    }

    fn run_systems(&mut self, world: &mut World) {
        escape_key_exit_system(world);
        pan_orbit_camera_system(world);
        self.atmosphere_switch_system(world);

        if self.loaded && self.rotation_speed > 0.0 {
            for entity in &self.model_entities {
                if let Some(transform) = world.get_local_transform_mut(*entity) {
                    let rotation = nalgebra_glm::quat_angle_axis(
                        self.rotation_speed * 0.016,
                        &nalgebra_glm::vec3(0.0, 1.0, 0.0),
                    );
                    transform.rotation = rotation * transform.rotation;
                }
                world.mark_local_transform_dirty(*entity);
            }
        }
    }

    fn on_dropped_file(&mut self, world: &mut World, path: &std::path::Path) {
        if let Some(extension) = path.extension() {
            let ext = extension.to_string_lossy().to_lowercase();
            if ext == "hdr" {
                self.load_hdr_skybox(world, path);
            } else if ext == "gltf" || ext == "glb" {
                self.load_gltf_from_path(world, path);
            }
            #[cfg(not(target_arch = "wasm32"))]
            if ext == "fbx" {
                self.load_fbx_animations(world, path);
            }
        }
        self.drag_file_type = None;
    }

    fn on_hovered_file(&mut self, _world: &mut World, path: &std::path::Path) {
        if let Some(extension) = path.extension() {
            let ext = extension.to_string_lossy().to_lowercase();
            if ext == "hdr" {
                self.drag_file_type = Some("HDR".to_string());
            } else if ext == "gltf" || ext == "glb" {
                self.drag_file_type = Some("glTF".to_string());
            } else if ext == "fbx" {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    self.drag_file_type = Some("FBX".to_string());
                }
                #[cfg(target_arch = "wasm32")]
                {
                    self.drag_file_type = Some("Unsupported".to_string());
                }
            } else {
                self.drag_file_type = Some("Unsupported".to_string());
            }
        }
    }

    fn on_hovered_file_cancelled(&mut self, _world: &mut World) {
        self.drag_file_type = None;
    }

    fn ui(&mut self, world: &mut World, ui_context: &egui::Context) {
        if self.drag_file_type.is_some() {
            self.drop_indicator_ui(ui_context);
        }

        egui::Window::new("Settings")
            .default_pos(egui::pos2(10.0, 10.0))
            .default_width(300.0)
            .show(ui_context, |ui| {
                ui.collapsing("Skybox", |ui| {
                    let current_is_default = self.selected_custom_skybox.is_none()
                        && world.resources.graphics.atmosphere == Atmosphere::Hdr;

                    if ui
                        .selectable_label(current_is_default, "Default (Moonrise)")
                        .clicked()
                    {
                        load_hdr_skybox(world, DEFAULT_HDR_BYTES.to_vec());
                        world.resources.graphics.atmosphere = Atmosphere::Hdr;
                        self.selected_custom_skybox = None;
                    }

                    for (index, skybox) in self.custom_skyboxes.iter().enumerate() {
                        let is_selected = self.selected_custom_skybox == Some(index);
                        if ui.selectable_label(is_selected, &skybox.name).clicked() {
                            load_hdr_skybox_from_path(world, skybox.path.clone());
                            world.resources.graphics.atmosphere = Atmosphere::Hdr;
                            self.selected_custom_skybox = Some(index);
                        }
                    }

                    ui.separator();

                    ui.label("Procedural Atmospheres:");
                    for atmosphere in Atmosphere::ALL {
                        if atmosphere.is_procedural() {
                            let is_selected = world.resources.graphics.atmosphere == *atmosphere
                                && self.selected_custom_skybox.is_none();
                            if ui
                                .selectable_label(is_selected, format!("{:?}", atmosphere))
                                .clicked()
                            {
                                world.resources.graphics.atmosphere = *atmosphere;
                                self.selected_custom_skybox = None;
                                capture_procedural_atmosphere_ibl(world, *atmosphere, 0.0);
                            }
                        }
                    }
                });

                ui.collapsing("Color Grading", |ui| {
                    let color_grading = &mut world.resources.graphics.color_grading;

                    ui.horizontal(|ui| {
                        ui.label("Preset:");
                        ui.label(color_grading.preset.name());
                    });

                    ui.horizontal_wrapped(|ui| {
                        for preset in ColorGradingPreset::ALL {
                            if *preset == ColorGradingPreset::Custom {
                                continue;
                            }
                            let is_selected = color_grading.preset == *preset;
                            if ui.selectable_label(is_selected, preset.name()).clicked() {
                                *color_grading = preset.to_color_grading();
                            }
                        }
                    });

                    ui.add_space(4.0);

                    ui.horizontal(|ui| {
                        ui.label("Tonemap:");
                        egui::ComboBox::from_id_salt("tonemap_algorithm")
                            .selected_text(color_grading.tonemap_algorithm.name())
                            .show_ui(ui, |ui| {
                                for algorithm in TonemapAlgorithm::ALL {
                                    if ui
                                        .selectable_value(
                                            &mut color_grading.tonemap_algorithm,
                                            *algorithm,
                                            algorithm.name(),
                                        )
                                        .changed()
                                    {
                                        color_grading.preset = ColorGradingPreset::Custom;
                                    }
                                }
                            });
                    });

                    ui.horizontal(|ui| {
                        ui.label("Gamma:");
                        if ui
                            .add(
                                egui::Slider::new(&mut color_grading.gamma, 1.0..=3.0)
                                    .fixed_decimals(2),
                            )
                            .changed()
                        {
                            color_grading.preset = ColorGradingPreset::Custom;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Saturation:");
                        if ui
                            .add(
                                egui::Slider::new(&mut color_grading.saturation, 0.0..=2.0)
                                    .fixed_decimals(2),
                            )
                            .changed()
                        {
                            color_grading.preset = ColorGradingPreset::Custom;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Brightness:");
                        if ui
                            .add(
                                egui::Slider::new(&mut color_grading.brightness, -0.5..=0.5)
                                    .fixed_decimals(2),
                            )
                            .changed()
                        {
                            color_grading.preset = ColorGradingPreset::Custom;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Contrast:");
                        if ui
                            .add(
                                egui::Slider::new(&mut color_grading.contrast, 0.5..=2.0)
                                    .fixed_decimals(2),
                            )
                            .changed()
                        {
                            color_grading.preset = ColorGradingPreset::Custom;
                        }
                    });
                });

                ui.collapsing("Model", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Rotation Speed:");
                        ui.add(
                            egui::Slider::new(&mut self.rotation_speed, 0.0..=2.0)
                                .fixed_decimals(2),
                        );
                    });

                    if ui.button("Reset Camera").clicked() {
                        self.reset_camera(world);
                    }
                });

                self.animation_ui(world, ui);

                ui.collapsing("Post Processing", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Bloom:");
                        ui.checkbox(&mut world.resources.graphics.bloom_enabled, "Enabled");
                    });

                    if world.resources.graphics.bloom_enabled {
                        ui.add(
                            egui::Slider::new(
                                &mut world.resources.graphics.bloom_intensity,
                                0.0..=0.1,
                            )
                            .step_by(0.01)
                            .fixed_decimals(2)
                            .text("Intensity"),
                        );
                    }

                    ui.horizontal(|ui| {
                        ui.label("SSAO:");
                        ui.checkbox(&mut world.resources.graphics.ssao_enabled, "Enabled");
                    });

                    if world.resources.graphics.ssao_enabled {
                        ui.add(
                            egui::Slider::new(&mut world.resources.graphics.ssao_radius, 0.1..=2.0)
                                .text("Radius"),
                        );
                        ui.add(
                            egui::Slider::new(&mut world.resources.graphics.ssao_bias, 0.001..=0.1)
                                .text("Bias"),
                        );
                        ui.add(
                            egui::Slider::new(
                                &mut world.resources.graphics.ssao_intensity,
                                0.5..=3.0,
                            )
                            .text("Intensity"),
                        );
                    }
                });

                ui.collapsing("Debug", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("PBR Debug:");
                        egui::ComboBox::from_id_salt("pbr_debug")
                            .selected_text(world.resources.graphics.pbr_debug_mode.name())
                            .show_ui(ui, |ui| {
                                for mode in PbrDebugMode::ALL {
                                    ui.selectable_value(
                                        &mut world.resources.graphics.pbr_debug_mode,
                                        *mode,
                                        mode.name(),
                                    );
                                }
                            });
                    });

                    ui.horizontal(|ui| {
                        ui.label("Texture Stripes:");
                        ui.checkbox(
                            &mut world.resources.graphics.texture_debug_stripes,
                            "Enabled",
                        );
                    });

                    if world.resources.graphics.texture_debug_stripes {
                        ui.horizontal(|ui| {
                            ui.label("Stripe Speed:");
                            ui.add(
                                egui::Slider::new(
                                    &mut world.resources.graphics.texture_debug_stripes_speed,
                                    0.0..=500.0,
                                )
                                .suffix(" px/s"),
                            );
                        });
                    }

                    ui.checkbox(&mut world.resources.graphics.show_grid, "Show Grid");
                });
            });
    }
}

impl ViewerState {
    fn drop_indicator_ui(&self, ui_context: &egui::Context) {
        egui::Area::new(egui::Id::new("drop_indicator"))
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui_context, |ui| {
                let frame = egui::Frame::default()
                    .fill(egui::Color32::from_rgba_premultiplied(30, 30, 30, 220))
                    .corner_radius(12.0)
                    .stroke(egui::Stroke::new(
                        2.0,
                        egui::Color32::from_rgb(100, 150, 250),
                    ))
                    .inner_margin(40.0);

                frame.show(ui, |ui| {
                    if let Some(file_type) = &self.drag_file_type {
                        match file_type.as_str() {
                            "HDR" => {
                                ui.heading("Drop HDR file to use as skybox");
                            }
                            "glTF" => {
                                ui.heading("Drop glTF/GLB file to load model");
                            }
                            "FBX" => {
                                ui.heading("Drop FBX file to add animations");
                            }
                            _ => {
                                ui.heading("Unsupported file type");
                            }
                        }
                    }
                });
            });
    }

    fn animation_ui(&mut self, world: &mut World, ui: &mut egui::Ui) {
        let animation_entity = self.model_entities.first().copied();
        let Some(entity) = animation_entity else {
            return;
        };

        if !world.entity_has_animation_player(entity) {
            return;
        }

        let mut clip_to_play = None;
        let mut clear_animations = false;

        ui.collapsing("Animation", |ui| {
            if let Some(player) = world.get_animation_player_mut(entity) {
                if player.clips.is_empty() {
                    ui.label("No animations (drop FBX to add)");
                    return;
                }

                ui.label(format!("{} clip(s) loaded", player.clips.len()));

                ui.horizontal(|ui| {
                    ui.label("Clip:");
                    egui::ComboBox::from_id_salt("animation_clip")
                        .selected_text(
                            player
                                .current_clip
                                .and_then(|index| player.clips.get(index))
                                .map(|clip| clip.name.as_str())
                                .unwrap_or("None"),
                        )
                        .show_ui(ui, |ui| {
                            for (index, clip) in player.clips.iter().enumerate() {
                                let is_selected = player.current_clip == Some(index);
                                if ui.selectable_label(is_selected, &clip.name).clicked() {
                                    clip_to_play = Some(index);
                                }
                            }
                        });
                });

                if let Some(clip_index) = player.current_clip
                    && let Some(clip) = player.clips.get(clip_index)
                {
                    ui.label(format!("Duration: {:.2}s", clip.duration));

                    ui.horizontal(|ui| {
                        ui.label("Time:");
                        ui.add(
                            egui::Slider::new(&mut player.time, 0.0..=clip.duration)
                                .fixed_decimals(2)
                                .suffix("s"),
                        );
                    });
                }

                ui.horizontal(|ui| {
                    ui.label("Speed:");
                    ui.add(
                        egui::DragValue::new(&mut player.speed)
                            .speed(0.1)
                            .range(-5.0..=5.0)
                            .fixed_decimals(1),
                    );
                });

                ui.checkbox(&mut player.looping, "Loop");

                ui.horizontal(|ui| {
                    if player.playing {
                        if ui.button("Pause").clicked() {
                            player.pause();
                        }
                    } else if ui.button("Play").clicked() {
                        player.resume();
                    }

                    if ui.button("Stop").clicked() {
                        player.stop();
                    }
                });

                ui.separator();

                if ui.button("Clear Animations").clicked() {
                    clear_animations = true;
                }
            }
        });

        if let Some(index) = clip_to_play
            && let Some(player) = world.get_animation_player_mut(entity)
        {
            player.play(index);
        }

        if clear_animations
            && let Some(player) = world.get_animation_player_mut(entity)
        {
            player.clips.clear();
            player.current_clip = None;
            player.playing = false;
            player.time = 0.0;
        }
    }

    fn atmosphere_switch_system(&mut self, world: &mut World) {
        let right_pressed = world
            .resources
            .input
            .keyboard
            .is_key_pressed(KeyCode::ArrowRight);
        let left_pressed = world
            .resources
            .input
            .keyboard
            .is_key_pressed(KeyCode::ArrowLeft);

        if right_pressed && !self.right_arrow_was_pressed {
            world.resources.graphics.atmosphere = world.resources.graphics.atmosphere.next();
            self.selected_custom_skybox = None;
        }
        if left_pressed && !self.left_arrow_was_pressed {
            world.resources.graphics.atmosphere = world.resources.graphics.atmosphere.previous();
            self.selected_custom_skybox = None;
        }

        self.right_arrow_was_pressed = right_pressed;
        self.left_arrow_was_pressed = left_pressed;

        let current_atmosphere = world.resources.graphics.atmosphere;
        if current_atmosphere != self.previous_atmosphere {
            if current_atmosphere.is_procedural() {
                capture_procedural_atmosphere_ibl(world, current_atmosphere, 0.0);
            }
            self.previous_atmosphere = current_atmosphere;
        }
    }

    fn load_hdr_skybox(&mut self, world: &mut World, path: &std::path::Path) {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Custom HDR")
            .to_string();

        let already_loaded = self.custom_skyboxes.iter().position(|s| s.path == path);

        if let Some(index) = already_loaded {
            load_hdr_skybox_from_path(world, path.to_path_buf());
            world.resources.graphics.atmosphere = Atmosphere::Hdr;
            self.selected_custom_skybox = Some(index);
        } else {
            load_hdr_skybox_from_path(world, path.to_path_buf());
            world.resources.graphics.atmosphere = Atmosphere::Hdr;

            self.custom_skyboxes.push(CustomSkybox {
                name,
                path: path.to_path_buf(),
            });
            self.selected_custom_skybox = Some(self.custom_skyboxes.len() - 1);
        }
    }

    fn load_gltf_from_path(&mut self, world: &mut World, path: &std::path::Path) {
        match nightshade::ecs::prefab::import_gltf_from_path(path) {
            Ok(result) => {
                self.clear_scene(world);
                self.process_gltf_result(world, result);
            }
            Err(error) => {
                tracing::error!("Failed to load glTF file: {}", error);
            }
        }
    }

    fn load_gltf_from_bytes(&mut self, world: &mut World, data: &[u8]) {
        match nightshade::ecs::prefab::import_gltf_from_bytes(data) {
            Ok(result) => {
                self.process_gltf_result(world, result);
            }
            Err(error) => {
                tracing::error!("Failed to load glTF from bytes: {}", error);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn load_fbx_animations(&mut self, world: &mut World, path: &std::path::Path) {
        let Some(entity) = self.model_entities.first().copied() else {
            tracing::warn!("No model loaded - load a glTF model first before adding FBX animations");
            return;
        };

        if !world.entity_has_animation_player(entity) {
            tracing::warn!("Model does not have an AnimationPlayer component");
            return;
        }

        match nightshade::ecs::prefab::import_fbx_from_path(path) {
            Ok(result) => {
                if result.animations.is_empty() {
                    tracing::warn!("No animations found in FBX file");
                    return;
                }

                if let Some(player) = world.get_animation_player_mut(entity) {
                    let count = result.animations.len();
                    player.add_clips(result.animations);
                    tracing::info!("Added {} animation(s) from FBX", count);

                    if player.current_clip.is_none() && !player.clips.is_empty() {
                        player.play(0);
                    }
                }
            }
            Err(error) => {
                tracing::error!("Failed to load FBX file: {}", error);
            }
        }
    }

    fn process_gltf_result(
        &mut self,
        world: &mut World,
        result: nightshade::ecs::prefab::GltfLoadResult,
    ) {
        for (name, (rgba_data, width, height)) in result.textures {
            world.queue_command(WorldCommand::LoadTexture {
                name,
                rgba_data,
                width,
                height,
            });
        }

        for (name, mesh) in result.meshes {
            mesh_cache_insert(&mut world.resources.mesh_cache, name, mesh);
        }

        for prefab in result.prefabs {
            let entity = nightshade::ecs::prefab::spawn_prefab_with_skins(
                world,
                &prefab,
                &result.animations,
                &result.skins,
                nalgebra_glm::vec3(0.0, 0.0, 0.0),
            );
            self.model_entities.push(entity);
        }

        self.loaded = true;
        self.center_and_fit_model(world);
    }

    fn clear_scene(&mut self, world: &mut World) {
        let entities: Vec<Entity> = self.model_entities.drain(..).collect();
        for entity in entities {
            despawn_recursive_immediate(world, entity);
        }
        self.loaded = false;
    }

    fn center_and_fit_model(&mut self, world: &mut World) {
        let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
        let mut has_bounds = false;

        for entity in &self.model_entities {
            calculate_bounds_recursive(
                world,
                *entity,
                &Mat4::identity(),
                &mut min,
                &mut max,
                &mut has_bounds,
            );
        }

        if !has_bounds {
            return;
        }

        let center = (min + max) * 0.5;
        let size = max - min;
        let max_dimension = size.x.max(size.y).max(size.z);

        if max_dimension <= 0.0 {
            return;
        }

        let scale = 2.0 / max_dimension;

        for entity in &self.model_entities {
            if let Some(transform) = world.get_local_transform_mut(*entity) {
                transform.translation = (transform.translation - center) * scale;
                transform.scale *= scale;
            }
            world.mark_local_transform_dirty(*entity);
        }

        self.reset_camera(world);
    }

    fn reset_camera(&mut self, world: &mut World) {
        if let Some(camera_entity) = self.camera_entity
            && let Some(pan_orbit) = world.get_pan_orbit_camera_mut(camera_entity)
        {
            pan_orbit.focus = Vec3::new(0.0, 0.0, 0.0);
            pan_orbit.target_focus = Vec3::new(0.0, 0.0, 0.0);
            pan_orbit.radius = 5.0;
            pan_orbit.target_radius = 5.0;
            pan_orbit.yaw = 0.0;
            pan_orbit.target_yaw = 0.0;
            pan_orbit.pitch = 0.3;
            pan_orbit.target_pitch = 0.3;
        }
    }
}

fn calculate_bounds_recursive(
    world: &World,
    entity: Entity,
    parent_transform: &Mat4,
    min: &mut Vec3,
    max: &mut Vec3,
    has_bounds: &mut bool,
) {
    let local_matrix = world
        .get_local_transform(entity)
        .map(|t| {
            nalgebra_glm::translation(&t.translation)
                * nalgebra_glm::quat_to_mat4(&t.rotation)
                * nalgebra_glm::scaling(&t.scale)
        })
        .unwrap_or_else(Mat4::identity);

    let global_matrix = parent_transform * local_matrix;

    if world.get_render_mesh(entity).is_some() {
        let position =
            nalgebra_glm::vec4_to_vec3(&(global_matrix * nalgebra_glm::vec4(0.0, 0.0, 0.0, 1.0)));
        *min = nalgebra_glm::min2(min, &position);
        *max = nalgebra_glm::max2(max, &position);
        *has_bounds = true;
    }

    let children: Vec<Entity> = world
        .resources
        .children_cache
        .get(&entity)
        .cloned()
        .unwrap_or_default();

    for child in children {
        calculate_bounds_recursive(world, child, &global_matrix, min, max, has_bounds);
    }
}
