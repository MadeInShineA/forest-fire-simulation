use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::input::ButtonInput;
use bevy::math::primitives::{Cuboid, Cylinder, Sphere};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPlugin};
use egui_plot::{Legend, Line, Plot, PlotPoints};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

// ─────────────────────────── Data structures ────────────────────────────

#[derive(Deserialize, Clone)]
struct GridData {
    width: usize,
    height: usize,
    steps: Vec<Vec<Vec<String>>>,
}
#[derive(Resource)]
struct Simulation {
    frames: Vec<Vec<Vec<String>>>,
    current: usize,
    width: usize,
    height: usize,
}
#[derive(Resource, Default)]
struct FrameTimer(Timer);
#[derive(Resource, Default)]
struct SimulationParams {
    width: u32,
    height: u32,
    burning_trees: u32,
    burning_grasses: u32,
    is_wind_toggled: bool,
    wind_angle: u32,
    wind_strength: u32,
    number_of_steps: u32,
    trigger_simulation: bool,
}
#[derive(Resource)]
struct LoadingScreen(bool);
#[derive(Resource)]
struct CachedAssets {
    meshes: HashMap<&'static str, Handle<Mesh>>,
    materials: HashMap<&'static str, Handle<StandardMaterial>>,
}
#[derive(Resource, Default)]
struct PendingSimulation {
    handle: Option<thread::JoinHandle<()>>,
    result: Arc<Mutex<Option<GridData>>>,
}
#[derive(Resource, Default)]
struct PlaybackControl {
    paused: bool,
    step_forward: bool,
    step_back: bool,
    go_to_start: bool,
    go_to_end: bool,
    speed: f32,
    jump_to_frame: Option<usize>,
}
#[derive(Resource, Clone)]
struct SimulationStats {
    trees_over_time: Vec<i64>,
    burning_trees_over_time: Vec<i64>,
    tree_ashes_over_time: Vec<i64>,
    grasses_over_time: Vec<i64>,
    burning_grasses_over_time: Vec<i64>,
    grass_ashes_over_time: Vec<i64>,
}
impl SimulationStats {
    // Modify the `new` function to potentially take initial frame data
    fn new(total_frames: usize, initial_stats: Option<(i64, i64, i64, i64, i64, i64)>) -> Self {
        let mut stats = Self {
            trees_over_time: vec![0; total_frames],
            burning_trees_over_time: vec![0; total_frames],
            tree_ashes_over_time: vec![0; total_frames],
            grasses_over_time: vec![0; total_frames],
            burning_grasses_over_time: vec![0; total_frames],
            grass_ashes_over_time: vec![0; total_frames],
        };

        if let Some((t, bt, ta, g, bg, ga)) = initial_stats {
            stats.trees_over_time[0] = t;
            stats.burning_trees_over_time[0] = bt;
            stats.tree_ashes_over_time[0] = ta;
            stats.grasses_over_time[0] = g;
            stats.burning_grasses_over_time[0] = bg;
            stats.grass_ashes_over_time[0] = ga;
        }
        stats
    }
}
#[derive(Component)]
struct CellEntity;
#[derive(Component)]
struct MainCamera;
#[derive(Component)]
struct SimulationEntity;
#[derive(Resource)]
struct LoadingTextTimer {
    timer: Timer,
    dot_count: usize,
}
#[derive(Component)]
struct FlyCamera;
// ───────────────────────────── App entry ────────────────────────────────
fn main() {
    App::new()
        .insert_resource(ClearColor(Color::rgb(0.05, 0.05, 0.1)))
        .insert_resource(SimulationParams {
            width: 20,
            height: 20,
            burning_trees: 15,
            burning_grasses: 20,
            is_wind_toggled: false,
            wind_angle: 0,
            wind_strength: 1,
            number_of_steps: 20,
            trigger_simulation: false,
        })
        .insert_resource(FrameTimer(Timer::from_seconds(0.4, TimerMode::Repeating)))
        .insert_resource(PlaybackControl {
            speed: 0.4,
            ..default()
        })
        .insert_resource(LoadingScreen(false))
        .insert_resource(PendingSimulation::default())
        .insert_resource(LoadingTextTimer {
            timer: Timer::from_seconds(0.5, TimerMode::Repeating),
            dot_count: 0,
        })
        // placeholder stats so systems can run before first sim loads
        .insert_resource(SimulationStats::new(1, None))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "🔥 Forest Fire Simulation 3D".into(),
                resolution: (1280., 800.).into(),
                resizable: false,
                // mode: bevy::window::WindowMode::Fullscreen,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin)
        .add_systems(Startup, setup_assets)
        .add_systems(
            Update,
            (
                ui_system,
                start_simulation_button_system,
                check_simulation_ready_system,
                advance_frame_system,
            ),
        )
        .add_systems(Update, camera_movement_system)
        .run();
}
fn camera_movement_system(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut mouse_motion_events: EventReader<MouseMotion>,
    mut scroll: EventReader<MouseWheel>,
    mut query: Query<&mut Transform, With<FlyCamera>>,
) {
    let mut transform = match query.get_single_mut() {
        Ok(t) => t,
        Err(_) => return,
    };
    // ─────── Movement ───────
    let mut direction = Vec3::ZERO;
    let forward: Vec3 = transform.forward().into();
    let right: Vec3 = transform.right().into();
    let up = Vec3::Y;
    let speed = 200.0 * time.delta_seconds();
    if keys.pressed(KeyCode::KeyW) {
        direction += forward;
    }
    if keys.pressed(KeyCode::KeyS) {
        direction -= forward;
    }
    if keys.pressed(KeyCode::KeyA) {
        direction -= right;
    }
    if keys.pressed(KeyCode::KeyD) {
        direction += right;
    }
    if keys.pressed(KeyCode::KeyE) {
        direction += up;
    }
    if keys.pressed(KeyCode::KeyQ) {
        direction -= up;
    }
    transform.translation += direction * speed;
    for ev in scroll.read() {
        transform.translation += forward * ev.y * 20.0;
    }
    // ─────── Rotation ───────
    if buttons.pressed(MouseButton::Left) {
        let mut delta = Vec2::ZERO;
        for ev in mouse_motion_events.read() {
            delta += ev.delta;
        }
        if delta.length_squared() > 0.0 {
            let yaw = Quat::from_rotation_y(-delta.x * 0.002);
            let pitch = Quat::from_rotation_x(-delta.y * 0.002);
            transform.rotation = yaw * transform.rotation; // yaw around global Y
            transform.rotation = transform.rotation * pitch; // pitch around local X
        }
    }
}
// ───────────────────────────── Asset setup ─────────────────────────────
fn setup_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut mesh_map = HashMap::new();
    mesh_map.insert("trunk", meshes.add(Mesh::from(Cylinder::new(1.0, 4.0))));
    mesh_map.insert("leaves", meshes.add(Mesh::from(Sphere::new(4.0))));
    mesh_map.insert("ash", meshes.add(Mesh::from(Cuboid::new(10.0, 0.5, 10.0))));
    mesh_map.insert("grass", meshes.add(Mesh::from(Cylinder::new(5.0, 0.5))));
    mesh_map.insert(
        "water",
        meshes.add(Mesh::from(Cuboid::new(10.0, 0.8, 10.0))),
    );
    mesh_map.insert(
        "burning_grass",
        meshes.add(Mesh::from(Cylinder::new(5.0, 0.5))),
    );
    mesh_map.insert(
        "burnt_grass",
        meshes.add(Mesh::from(Cylinder::new(5.0, 0.2))),
    );
    mesh_map.insert("burning_leaves", mesh_map["leaves"].clone());
    let mut mat_map = HashMap::new();
    mat_map.insert(
        "trunk",
        materials.add(StandardMaterial {
            base_color: Color::rgb(0.4, 0.25, 0.1),
            ..default()
        }),
    );
    mat_map.insert(
        "leaves",
        materials.add(StandardMaterial {
            base_color: Color::rgb(0.1, 0.6, 0.1),
            ..default()
        }),
    );
    mat_map.insert(
        "burning_leaves",
        materials.add(StandardMaterial {
            base_color: Color::rgb(0.8, 0.1, 0.0),
            emissive: Color::rgb(3.0, 0.6, 0.3),
            ..default()
        }),
    );
    mat_map.insert(
        "ash",
        materials.add(StandardMaterial {
            base_color: Color::rgb(0.2, 0.2, 0.2),
            ..default()
        }),
    );
    mat_map.insert(
        "grass",
        materials.add(StandardMaterial {
            base_color: Color::rgb(0.1, 0.8, 0.1),
            ..default()
        }),
    );
    mat_map.insert(
        "water",
        materials.add(StandardMaterial {
            base_color: Color::rgb(0.1, 0.3, 0.8),
            reflectance: 0.8,
            perceptual_roughness: 0.3,
            ..default()
        }),
    );
    mat_map.insert(
        "burning_grass",
        materials.add(StandardMaterial {
            base_color: Color::rgb(0.9, 0.2, 0.0),
            emissive: Color::rgb(3.0, 1.2, 0.3),
            ..default()
        }),
    );
    mat_map.insert(
        "burnt_grass",
        materials.add(StandardMaterial {
            base_color: Color::rgb(0.3, 0.3, 0.3),
            ..default()
        }),
    );
    commands.insert_resource(CachedAssets {
        meshes: mesh_map,
        materials: mat_map,
    });
}
// ───────────────────────── Simulation start ────────────────────────────
fn start_simulation_button_system(
    mut commands: Commands,
    mut params: ResMut<SimulationParams>,
    mut loading: ResMut<LoadingScreen>,
    mut pending: ResMut<PendingSimulation>,
    old_entities: Query<Entity, With<SimulationEntity>>,
) {
    if !params.trigger_simulation || loading.0 {
        return;
    }
    for e in old_entities.iter() {
        commands.entity(e).despawn_recursive();
    }
    loading.0 = true;
    let result = Arc::new(Mutex::new(None));
    let cmd = format!(
        "sh run-sim.sh {} {} {} {} {} {} {} {}",
        params.width,
        params.height,
        params.burning_trees,
        params.burning_grasses,
        params.is_wind_toggled as i32,
        params.wind_angle,
        params.wind_strength,
        params.number_of_steps
    );
    let result_clone = Arc::clone(&result);
    let handle = thread::spawn(move || {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .stdout(Stdio::piped())
            .spawn()
            .expect("failed to launch Scala sim");
        if let Some(stdout) = child.stdout.take() {
            for line in BufReader::new(stdout).lines().flatten() {
                println!("[Scala]: {}", line);
            }
        }
        let _ = child.wait();
        if let Some(data) = load_simulation_data() {
            *result_clone.lock().unwrap() = Some(data);
        }
    });
    pending.handle = Some(handle);
    pending.result = result;
    params.trigger_simulation = false;
}
// ───────────────────────── Spawn camera & lights ─────────────────────────
fn spawn_camera(commands: &mut Commands) {
    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_xyz(0.0, 250.0, 400.0).looking_at(Vec3::ZERO, Vec3::Y),
            ..default()
        },
        MainCamera,
        FlyCamera,
        SimulationEntity,
    ));
    commands.spawn((
        DirectionalLightBundle {
            transform: Transform::from_xyz(0.0, 200.0, 100.0).looking_at(Vec3::ZERO, Vec3::Y),
            directional_light: DirectionalLight {
                shadows_enabled: false,
                illuminance: 10000.0,
                ..default()
            },
            ..default()
        },
        SimulationEntity,
    ));
    commands.spawn((
        PointLightBundle {
            transform: Transform::from_xyz(100.0, 150.0, 100.0),
            point_light: PointLight {
                intensity: 5000.0,
                range: 500.0,
                shadows_enabled: false,
                ..default()
            },
            ..default()
        },
        SimulationEntity,
    ));
    commands.insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 0.2,
    });
}
// ───────────────────────── Check sim ready ─────────────────────────────
fn check_simulation_ready_system(
    mut commands: Commands,
    mut loading: ResMut<LoadingScreen>,
    mut pending: ResMut<PendingSimulation>,
    mut playback: ResMut<PlaybackControl>,
) {
    if !loading.0 {
        return;
    }
    // Scope the immutable borrow of pending.result so it ends before we do `*pending = …`
    let data_opt = {
        let guard = pending.result.lock().unwrap();
        guard.clone()
    };
    if let Some(data) = data_opt {
        let total_frames = data.steps.len();
        let first_frame_data = &data.steps[0];

        // Calculate initial stats for frame 0
        let (mut t, mut bt, mut ta, mut g, mut bg, mut ga) = (0, 0, 0, 0, 0, 0);
        for row in first_frame_data.iter() {
            for cell in row.iter() {
                match cell.as_str() {
                    "T" => t += 1,
                    "*" => bt += 1,
                    "G" => g += 1,
                    "A" => ta += 1,
                    "+" => bg += 1,
                    "-" => ga += 1,
                    _ => {}
                }
            }
        }

        commands.insert_resource(SimulationStats::new(
            total_frames,
            Some((t, bt, ta, g, bg, ga)),
        ));

        let frames = data.steps;
        // start at index 0
        let start_frame = 0;
        commands.insert_resource(Simulation {
            frames,
            current: start_frame,
            width: data.width,
            height: data.height,
        });
        spawn_camera(&mut commands);
        playback.paused = false;
        playback.jump_to_frame = Some(0);
        loading.0 = false;
        *pending = PendingSimulation::default();
    }
}
// ─────────────────────── Frame advance system ────────────────────────
fn advance_frame_system(
    mut commands: Commands,
    time: Res<Time>,
    mut timer: ResMut<FrameTimer>,
    mut sim: Option<ResMut<Simulation>>,
    mut playback: ResMut<PlaybackControl>,
    cells: Query<Entity, With<CellEntity>>,
    cache: Res<CachedAssets>,
    mut stats: ResMut<SimulationStats>,
) {
    let mut sim = match sim.as_mut() {
        Some(s) => s,
        None => return,
    };
    // 1) update timer to match speed slider
    if timer.0.duration().as_secs_f32() != playback.speed {
        timer
            .0
            .set_duration(std::time::Duration::from_secs_f32(playback.speed));
    }
    // 2) bail if paused + no explicit step/jump
    if playback.paused
        && !playback.step_forward
        && !playback.step_back
        && playback.jump_to_frame.is_none()
    {
        return;
    }
    // 3) require either a timer tick or an explicit step/jump
    let ticked = timer.0.tick(time.delta()).just_finished();
    if !ticked && playback.jump_to_frame.is_none() && !playback.step_forward && !playback.step_back
    {
        return;
    }
    // save old index for stats
    let prev = sim.current;
    let last = sim.frames.len() - 1;
    // 4) decide the next index
    let mut next = sim.current;
    if let Some(jump) = playback.jump_to_frame.take() {
        next = jump.min(last);
    } else if playback.step_back {
        if next > 0 {
            next -= 1;
        }
        playback.step_back = false;
    } else if playback.step_forward {
        next = (next + 1).min(last);
        playback.step_forward = false;
    } else if !playback.paused && ticked {
        if next < last {
            next += 1;
        } else {
            // wrap-around at end
            next = 0;
            playback.paused = true;
        }
    }
    sim.current = next;
    // 5) despawn old frame
    for ent in cells.iter() {
        commands.entity(ent).despawn_recursive();
    }
    // 6) render the new current frame
    let grid = &sim.frames[sim.current];
    let cell_size = 10.0;
    let spacing = 1.5;
    let offset_x = -(sim.width as f32 * cell_size * spacing) / 2.0;
    let offset_z = -(sim.height as f32 * cell_size * spacing) / 2.0;
    let (mut t, mut bt, mut ta, mut g, mut bg, mut ga) = (0, 0, 0, 0, 0, 0);
    for (y, row) in grid.iter().enumerate() {
        for (x, cell) in row.iter().enumerate() {
            let pos = Vec3::new(
                offset_x + x as f32 * cell_size * spacing,
                0.0,
                offset_z + y as f32 * cell_size * spacing,
            );
            match cell.as_str() {
                "T" | "*" => {
                    if cell == "*" {
                        bt += 1
                    } else {
                        t += 1
                    }
                    spawn_cell(&mut commands, &cache, "trunk", pos + Vec3::Y * 2.0);
                    spawn_cell(
                        &mut commands,
                        &cache,
                        if cell == "*" {
                            "burning_leaves"
                        } else {
                            "leaves"
                        },
                        pos + Vec3::Y * 7.0,
                    );
                }
                other => {
                    match other {
                        "G" => g += 1,
                        "A" => ta += 1,
                        "+" => bg += 1,
                        "-" => ga += 1,
                        _ => {}
                    }
                    spawn_cell(
                        &mut commands,
                        &cache,
                        kind_from_str(other),
                        pos + Vec3::Y * 0.25,
                    );
                }
            }
        }
    }
    // 7) update stats only on forward motion within bounds, or if jumping to a frame
    //    We also need to update stats if the current frame is 0 and it was just loaded
    //    (handled by the initial stats in `check_simulation_ready_system`).
    //    For subsequent frames, update only when moving forward or explicitly jumping.
    if sim.current > prev || (sim.current == 0 && prev == 0 && sim.frames.len() > 1) {
        update_stats(&mut stats, sim.current, t, bt, ta, g, bg, ga);
    }
}
fn update_stats(
    stats: &mut SimulationStats,
    idx: usize,
    trees: i64,
    burning_trees: i64,
    tree_ashes: i64,
    grasses: i64,
    burning_grasses: i64,
    grass_ashes: i64,
) {
    // Only update the specific index
    stats.trees_over_time[idx] = trees;
    stats.burning_trees_over_time[idx] = burning_trees;
    stats.tree_ashes_over_time[idx] = tree_ashes;
    stats.grasses_over_time[idx] = grasses;
    stats.burning_grasses_over_time[idx] = burning_grasses;
    stats.grass_ashes_over_time[idx] = grass_ashes;
}
fn kind_from_str(cell: &str) -> &'static str {
    match cell {
        "G" => "grass",
        "A" => "ash",
        "W" => "water",
        "+" => "burning_grass",
        "-" => "burnt_grass",
        _ => "ash",
    }
}
fn spawn_cell(commands: &mut Commands, cache: &CachedAssets, kind: &str, pos: Vec3) {
    commands.spawn((
        PbrBundle {
            mesh: cache.meshes[kind].clone(),
            material: cache.materials[kind].clone(),
            transform: Transform::from_translation(pos),
            ..default()
        },
        CellEntity,
        SimulationEntity,
    ));
}
// ─────────────────────────── UI system ────────────────────────────────
fn ui_system(
    mut contexts: EguiContexts,
    mut params: ResMut<SimulationParams>,
    sim: Option<Res<Simulation>>,
    loading: Res<LoadingScreen>,
    mut text_timer: ResMut<LoadingTextTimer>,
    time: Res<Time>,
    mut playback: ResMut<PlaybackControl>,
    stats: Res<SimulationStats>,
) {
    let sim_ref = sim.as_ref().map(|r| &**r);
    let ctx = contexts.ctx_mut();
    // ─────── Loading overlay ─────────────────────────────────────
    if loading.0 {
        text_timer.timer.tick(time.delta());
        if text_timer.timer.just_finished() {
            text_timer.dot_count = (text_timer.dot_count + 1) % 4;
        }
        let dots = ".".repeat(text_timer.dot_count);
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading("⌛ Generating Simulation");
                ui.label(format!("Loading{}", dots));
            });
        });
        return;
    }
    // ─────── Side panel with controls ────────────────────────────
    egui::SidePanel::left("side_panel")
        .resizable(true)
        .min_width(200.0)
        .show(ctx, |ui| {
            ui.heading("Simulation Controls");
            ui.separator();
            // Parameter sliders
            ui.add(egui::Slider::new(&mut params.width, 10..=100).text("Width"));
            ui.add(egui::Slider::new(&mut params.height, 10..=100).text("Height"));
            ui.add(egui::Slider::new(&mut params.burning_trees, 0..=100).text("Burning trees %"));
            ui.add(
                egui::Slider::new(&mut params.burning_grasses, 0..=100).text("Burning grasses %"),
            );
            ui.add(egui::Slider::new(&mut params.number_of_steps, 1..=100).text("Number of steps"));
            ui.add(egui::Checkbox::new(
                &mut params.is_wind_toggled,
                "Enable wind",
            ));
            if params.is_wind_toggled {
                ui.add(egui::Slider::new(&mut params.wind_angle, 0..=359).text("Wind angle °"));
                ui.add(
                    egui::Slider::new(&mut params.wind_strength, 1..=100)
                        .text("Wind strength km/h"),
                );
            }
            if ui.button("Start Simulation").clicked() {
                params.trigger_simulation = true;
            }
            if let Some(sim) = sim_ref {
                ui.separator();
                ui.label("Playback Controls");
                ui.horizontal(|ui| {
                    // go-to-start (reset)
                    if ui.small_button("|⏮").clicked() {
                        playback.jump_to_frame = Some(0);
                        playback.paused = true;
                    }
                    if ui.small_button("⏮").clicked() {
                        playback.step_back = true;
                    }
                    if ui
                        .small_button(if playback.paused { "▶" } else { "⏸" })
                        .clicked()
                    {
                        playback.paused = !playback.paused;
                    }
                    if ui.small_button("⏭").clicked() {
                        playback.step_forward = true;
                    }
                    if ui.small_button("⏭|").clicked() {
                        playback.jump_to_frame = Some(sim.frames.len() - 1);
                    }
                });
                ui.add(egui::Slider::new(&mut playback.speed, 0.05..=2.0).text("Speed s/frame"));
                // Frame slider
                ui.label(format!("Frame: {}/{}", sim.current + 1, sim.frames.len()));
                let mut display_frame = sim.current + 1;
                if ui
                    .add(egui::Slider::new(&mut display_frame, 1..=sim.frames.len()).text("Frame"))
                    .changed()
                {
                    playback.jump_to_frame = Some(display_frame - 1);
                }
                ui.separator();

                let total_trees_over_time: Vec<f64> = (0..=sim.current)
                    .map(|i| {
                        (stats.trees_over_time[i]
                            + stats.burning_trees_over_time[i]
                            + stats.tree_ashes_over_time[i]) as f64
                    })
                    .collect();

                let total_grass_over_time: Vec<f64> = (0..=sim.current)
                    .map(|i| {
                        (stats.grasses_over_time[i]
                            + stats.burning_grasses_over_time[i]
                            + stats.grass_ashes_over_time[i]) as f64
                    })
                    .collect();

                ui.collapsing("Graphs", |ui| {
                    ui.label("Tree Status (%)");
                    Plot::new("Trees Percentage")
                        .legend(Legend::default())
                        .height(120.0)
                        .show(ui, |plot_ui| {
                            let trees: PlotPoints = (0..=sim.current)
                                .map(|i| {
                                    let total = total_trees_over_time[i];
                                    let value = if total > 0.0 {
                                        (stats.trees_over_time[i] as f64 / total) * 100.0
                                    } else {
                                        0.0
                                    };
                                    [i as f64, value]
                                })
                                .collect();
                            let burning: PlotPoints = (0..=sim.current)
                                .map(|i| {
                                    let total = total_trees_over_time[i];
                                    let value = if total > 0.0 {
                                        (stats.burning_trees_over_time[i] as f64 / total) * 100.0
                                    } else {
                                        0.0
                                    };
                                    [i as f64, value]
                                })
                                .collect();
                            let ashes: PlotPoints = (0..=sim.current)
                                .map(|i| {
                                    let total = total_trees_over_time[i];
                                    let value = if total > 0.0 {
                                        (stats.tree_ashes_over_time[i] as f64 / total) * 100.0
                                    } else {
                                        0.0
                                    };
                                    [i as f64, value]
                                })
                                .collect();

                            plot_ui.line(Line::new(trees).name("Trees %"));
                            plot_ui.line(Line::new(burning).name("Burning %"));
                            plot_ui.line(Line::new(ashes).name("Ashes %"));
                        });

                    // --- Grass Status as Percent ---
                    ui.label("Grass Status (%)");
                    Plot::new("Grass Percentage")
                        .legend(Legend::default())
                        .height(120.0)
                        .show(ui, |plot_ui| {
                            let grasses: PlotPoints = (0..=sim.current)
                                .map(|i| {
                                    let total = total_grass_over_time[i];
                                    let value = if total > 0.0 {
                                        (stats.grasses_over_time[i] as f64 / total) * 100.0
                                    } else {
                                        0.0
                                    };
                                    [i as f64, value]
                                })
                                .collect();
                            let burning: PlotPoints = (0..=sim.current)
                                .map(|i| {
                                    let total = total_grass_over_time[i];
                                    let value = if total > 0.0 {
                                        (stats.burning_grasses_over_time[i] as f64 / total)
                                            * 100.0
                                    } else {
                                        0.0
                                    };
                                    [i as f64, value]
                                })
                                .collect();
                            let ashes: PlotPoints = (0..=sim.current)
                                .map(|i| {
                                    let total = total_grass_over_time[i];
                                    let value = if total > 0.0 {
                                        (stats.grass_ashes_over_time[i] as f64 / total) * 100.0
                                    } else {
                                        0.0
                                    };
                                    [i as f64, value]
                                })
                                .collect();

                            plot_ui.line(Line::new(grasses).name("Grass %"));
                            plot_ui.line(Line::new(burning).name("Burning %"));
                            plot_ui.line(Line::new(ashes).name("Ashes %"));
                        });
                });
            }
        });

    // ───────────────── Wind Indicator (NEW) ──────────────────
    if params.is_wind_toggled {
        egui::Area::new("wind_indicator")
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-20.0, 20.0))
            .show(ctx, |ui| {
                let desired_size = egui::vec2(120.0, 140.0);
                let (rect, _response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());
                let painter = ui.painter();
                let center = rect.center() - egui::vec2(0.0, 10.0);

                // Draw compass background
                let compass_radius = 50.0;
                painter.circle_stroke(center, compass_radius, egui::Stroke::new(1.0, egui::Color32::GRAY));
                painter.text(center + egui::vec2(0.0, -(compass_radius + 5.0)), egui::Align2::CENTER_CENTER, "N", egui::FontId::proportional(12.0), egui::Color32::LIGHT_GRAY);
                painter.text(center + egui::vec2(0.0, compass_radius + 5.0), egui::Align2::CENTER_CENTER, "S", egui::FontId::proportional(12.0), egui::Color32::LIGHT_GRAY);
                painter.text(center + egui::vec2(compass_radius + 5.0, 0.0), egui::Align2::CENTER_CENTER, "E", egui::FontId::proportional(12.0), egui::Color32::LIGHT_GRAY);
                painter.text(center + egui::vec2(-(compass_radius + 5.0), 0.0), egui::Align2::CENTER_CENTER, "W", egui::FontId::proportional(12.0), egui::Color32::LIGHT_GRAY);

                // Calculate arrow properties
                // The angle from the slider indicates where the wind is COMING FROM.
                // The arrow should point in the direction the wind is GOING TO.
                // For 0 degrees to be North, and rotate clockwise for increasing angles:
                // Egui's +Y is down, +X is right.
                // North (0 deg) should be straight up (-Y).
                // East (90 deg) should be straight right (+X).
                // South (180 deg) should be straight down (+Y).
                // West (270 deg) should be straight left (-X).

                // Convert angle from degrees to radians, adjust for coordinate system.
                // 0 degrees North (up) in Egui is -PI/2 or 270 degrees in standard math unit circle.
                // Clockwise rotation means increasing angle reduces Y and increases X for first quadrant.
                let wind_goes_to_angle_rad = (params.wind_angle as f32).to_radians();

                // To make 0 degrees point North and rotate clockwise:
                // Egui's coordinate system: +X right, +Y down.
                // North: (0, -1)
                // East:  (1, 0)
                // South: (0, 1)
                // West:  (-1, 0)
                // We want:
                // Angle 0 (N) -> (0, -1)
                // Angle 90 (E) -> (1, 0)
                // Angle 180 (S) -> (0, 1)
                // Angle 270 (W) -> (-1, 0)

                // This mapping is achieved by `(sin(angle), cos(angle))` if 0 degrees is positive X
                // and positive angle is counter-clockwise.
                // Since our angle is already clockwise from North, we can directly use:
                let dir_x = wind_goes_to_angle_rad.sin();
                let dir_y = -wind_goes_to_angle_rad.cos(); // Negate cos to make North point up (-Y)

                let dir = egui::vec2(dir_x, dir_y);

                // Strength affects length (slider range 1-100)
                let max_len = compass_radius - 5.0;
                let min_len = 10.0;
                let strength_ratio = (params.wind_strength.saturating_sub(1)) as f32 / 99.0;
                let length = min_len + strength_ratio * (max_len - min_len);

                // Center the arrow within the compass
                let arrow_base = center - dir * length / 2.0;
                let arrow_vec = dir * length;

                painter.arrow(
                    arrow_base,
                    arrow_vec,
                    egui::Stroke::new(3.0, egui::Color32::from_rgb(255, 100, 100)),
                );

                // Draw strength text below the compass
                let strength_text = format!("{} km/h", params.wind_strength);
                painter.text(
                    rect.center() + egui::vec2(0.0, compass_radius + 5.0),
                    egui::Align2::CENTER_CENTER,
                    strength_text,
                    egui::FontId::proportional(12.0),
                    egui::Color32::LIGHT_GRAY,
                );
            });
    }
}
fn load_simulation_data() -> Option<GridData> {
    let file = File::open("assets/simulation.json").ok()?;
    serde_json::from_reader(BufReader::new(file)).ok()
}
