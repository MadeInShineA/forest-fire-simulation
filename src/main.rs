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
    progress: Arc<Mutex<f32>>,
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
    fn new(total_frames: usize) -> Self {
        Self {
            trees_over_time: vec![0; total_frames],
            burning_trees_over_time: vec![0; total_frames],
            tree_ashes_over_time: vec![0; total_frames],
            grasses_over_time: vec![0; total_frames],
            burning_grasses_over_time: vec![0; total_frames],
            grass_ashes_over_time: vec![0; total_frames],
        }
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

// ───────────────────────────── App entry ────────────────────────────────

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::rgb(0.05, 0.05, 0.1)))
        .insert_resource(SimulationParams {
            width: 20,
            height: 20,
            burning_trees: 15,
            burning_grasses: 20,
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
        .insert_resource(SimulationStats::new(1))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "🔥 Forest Fire Simulation 3D".into(),
                resolution: (1280., 800.).into(),
                resizable: false,
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
        .run();
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
    let progress = Arc::new(Mutex::new(0.0));
    let cmd = format!(
        "sh run-sim.sh {} {} {} {} {}",
        params.width,
        params.height,
        params.burning_trees,
        params.burning_grasses,
        params.number_of_steps
    );
    let result_clone = Arc::clone(&result);
    let progress_clone = Arc::clone(&progress);

    let handle = thread::spawn(move || {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .stdout(Stdio::piped())
            .spawn()
            .expect("failed to launch Scala sim");

        if let Some(stdout) = child.stdout.take() {
            for line in BufReader::new(stdout).lines().flatten() {
                if let Some(p) = line.strip_prefix("PROGRESS:") {
                    if let Ok(val) = p.trim().parse::<f32>() {
                        *progress_clone.lock().unwrap() = val.min(100.0);
                    }
                }
            }
        }

        let _ = child.wait();
        if let Some(data) = load_simulation_data() {
            *result_clone.lock().unwrap() = Some(data);
        }
    });

    pending.handle = Some(handle);
    pending.result = result;
    pending.progress = progress;
    params.trigger_simulation = false;
}

// ───────────────────────── Spawn camera & lights ─────────────────────────

fn spawn_camera(commands: &mut Commands) {
    // Camera
    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_xyz(0.0, 250.0, 400.0).looking_at(Vec3::ZERO, Vec3::Y),
            ..default()
        },
        MainCamera,
        SimulationEntity,
    ));
    // Disable shadows to reduce GPU memory usage
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
        commands.insert_resource(SimulationStats::new(total_frames));
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

        playback.paused = true;
        playback.jump_to_frame = Some(start_frame);

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
                        _ => ta += 1,
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

    // 7) update stats only on forward motion within bounds
    if sim.current > prev {
        update_stats(&mut stats, prev, sim.current, t, bt, ta, g, bg, ga);
    }
}

fn update_stats(
    stats: &mut SimulationStats,
    start: usize,
    end: usize,
    trees: i64,
    burning_trees: i64,
    tree_ashes: i64,
    grasses: i64,
    burning_grasses: i64,
    grass_ashes: i64,
) {
    for idx in start..=end {
        stats.trees_over_time[idx] = trees;
        stats.burning_trees_over_time[idx] = burning_trees;
        stats.tree_ashes_over_time[idx] = tree_ashes;
        stats.grasses_over_time[idx] = grasses;
        stats.burning_grasses_over_time[idx] = burning_grasses;
        stats.grass_ashes_over_time[idx] = grass_ashes;
    }
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

                // ─────── Graphs ────────────────────────────────────────
                ui.collapsing("Graphs", |ui| {
                    ui.label("Tree Status");
                    Plot::new("Trees")
                        .legend(Legend::default())
                        .height(120.0)
                        .show(ui, |plot_ui| {
                            let trees: PlotPoints = stats
                                .trees_over_time
                                .iter()
                                .take(sim.current + 1)
                                .enumerate()
                                .map(|(i, &v)| [i as f64, v as f64])
                                .collect();
                            let burning: PlotPoints = stats
                                .burning_trees_over_time
                                .iter()
                                .take(sim.current + 1)
                                .enumerate()
                                .map(|(i, &v)| [i as f64, v as f64])
                                .collect();
                            let ashes: PlotPoints = stats
                                .tree_ashes_over_time
                                .iter()
                                .take(sim.current + 1)
                                .enumerate()
                                .map(|(i, &v)| [i as f64, v as f64])
                                .collect();
                            plot_ui.line(Line::new(trees).name("Trees"));
                            plot_ui.line(Line::new(burning).name("Burning"));
                            plot_ui.line(Line::new(ashes).name("Ashes"));
                        });

                    ui.separator();
                    ui.label("Grass Status");
                    Plot::new("Grasses")
                        .legend(Legend::default())
                        .height(120.0)
                        .show(ui, |plot_ui| {
                            let grasses: PlotPoints = stats
                                .grasses_over_time
                                .iter()
                                .take(sim.current + 1)
                                .enumerate()
                                .map(|(i, &v)| [i as f64, v as f64])
                                .collect();
                            let burning: PlotPoints = stats
                                .burning_grasses_over_time
                                .iter()
                                .take(sim.current + 1)
                                .enumerate()
                                .map(|(i, &v)| [i as f64, v as f64])
                                .collect();
                            let ashes: PlotPoints = stats
                                .grass_ashes_over_time
                                .iter()
                                .take(sim.current + 1)
                                .enumerate()
                                .map(|(i, &v)| [i as f64, v as f64])
                                .collect();
                            plot_ui.line(Line::new(grasses).name("Grass"));
                            plot_ui.line(Line::new(burning).name("Burning"));
                            plot_ui.line(Line::new(ashes).name("Ashes"));
                        });
                });
            }
        });
}

fn load_simulation_data() -> Option<GridData> {
    let file = File::open("assets/simulation.json").ok()?;
    serde_json::from_reader(BufReader::new(file)).ok()
}
