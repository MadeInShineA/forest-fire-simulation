use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::input::ButtonInput;
use bevy::math::primitives::{Cuboid, Cylinder, Sphere};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPlugin};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

// --- Use crossbeam_channel for thread-safe Bevy resources ---
use crossbeam_channel::{unbounded, Receiver, Sender};

enum SimulationFrameMsg {
    Metadata { width: usize, height: usize },
    Frame(Vec<Vec<String>>),
    SimulationEnded,
}

// Channel as a Bevy resource (crossbeam_channel is Sync!)
#[derive(Resource)]
struct NdjsonChannel(pub Receiver<SimulationFrameMsg>);

#[derive(Resource)]
struct Simulation {
    frames: Vec<Vec<Vec<String>>>,
    current: usize,
    width: usize,
    height: usize,
}
#[derive(Resource, Default)]
struct FrameTimer(Timer);

#[derive(Resource, Default, Clone)]
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

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct SimControl {
    pub windAngle: Option<i32>,
    pub windStrength: Option<i32>,
    pub windEnabled: Option<bool>,
    pub paused: Option<bool>,
    pub step: Option<bool>,
}

const CONTROL_PATH: &str = "assets/sim_control.json";

// Reads current sim_control.json, or default if not found
fn read_sim_control() -> SimControl {
    if let Ok(content) = fs::read_to_string(CONTROL_PATH) {
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        SimControl::default()
    }
}

// Updates only the Some fields (merges)
fn update_sim_control(update: SimControl) {
    let mut control = read_sim_control();
    if let Some(val) = update.windAngle {
        control.windAngle = Some(val);
    }
    if let Some(val) = update.windStrength {
        control.windStrength = Some(val);
    }
    if let Some(val) = update.windEnabled {
        control.windEnabled = Some(val);
    }
    if let Some(val) = update.paused {
        control.paused = Some(val);
    }
    if let Some(val) = update.step {
        control.step = Some(val);
    }
    let json = serde_json::to_string_pretty(&control).unwrap();
    fs::write(CONTROL_PATH, json).expect("Failed to write sim_control.json");
}

#[derive(Resource, Default)]
struct PlaybackControl {
    paused: bool,
    step_forward: bool,
    step_back: bool,
    speed: f32,
    jump_to_frame: Option<usize>,
}

#[derive(Resource, Clone)]
struct SimulationStats {
    pub frame_counter: usize,
    trees_over_time: Vec<i64>,
    burning_trees_over_time: Vec<i64>,
    tree_ashes_over_time: Vec<i64>,
    grasses_over_time: Vec<i64>,
    burning_grasses_over_time: Vec<i64>,
    grass_ashes_over_time: Vec<i64>,
}
impl SimulationStats {
    fn new(total_frames: usize, initial_stats: Option<(i64, i64, i64, i64, i64, i64)>) -> Self {
        let mut stats = Self {
            frame_counter: 0, // <-- ADD THIS LINE
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
#[derive(Resource)]
struct CachedAssets {
    meshes: HashMap<&'static str, Handle<Mesh>>,
    materials: HashMap<&'static str, Handle<StandardMaterial>>,
}
#[derive(Component)]
struct CellEntity;
#[derive(Component)]
struct MainCamera;
#[derive(Component)]
struct SimulationEntity;
#[derive(Component)]
struct FlyCamera;
#[derive(Resource, Default)]
struct ShowGraphs(pub bool);
#[derive(Resource, Default)]
struct NdjsonTailingHandle(Option<thread::JoinHandle<()>>);
#[derive(Resource, Default)]
struct NdjsonKillSwitch(pub Arc<Mutex<bool>>);

#[derive(Resource)]
struct LoadingScreen(pub bool);

#[derive(Resource)]
struct LoadingTextTimer {
    timer: Timer,
    dot_count: usize,
}

// ------------- NDJSON Tailing Thread -------------
fn spawn_ndjson_tailer(
    tx: Sender<SimulationFrameMsg>,
    ndjson_path: String,
    kill_switch: Arc<Mutex<bool>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        // Wait for file to be created (simulate start)
        let file = loop {
            if *kill_switch.lock().unwrap() {
                return;
            }
            if let Ok(f) = File::open(&ndjson_path) {
                break f;
            }
            thread::sleep(Duration::from_millis(200));
        };
        let mut reader = BufReader::new(file);
        let mut line = String::new();
        let mut sent_metadata = false;
        loop {
            if *kill_switch.lock().unwrap() {
                return;
            }
            line.clear();
            let bytes = reader.read_line(&mut line).unwrap();
            if bytes == 0 {
                thread::sleep(Duration::from_millis(50));
                continue;
            }
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                continue;
            }
            println!("NDJSON line: {trimmed}");
            if !sent_metadata {
                let meta: serde_json::Value = serde_json::from_str(trimmed).unwrap();
                let width = meta["width"].as_u64().unwrap() as usize;
                let height = meta["height"].as_u64().unwrap() as usize;
                tx.send(SimulationFrameMsg::Metadata { width, height })
                    .unwrap();
                sent_metadata = true;
            } else {
                let frame: Vec<Vec<String>> = serde_json::from_str(trimmed).unwrap();
                tx.send(SimulationFrameMsg::Frame(frame)).unwrap();
            }
        }
    })
}

// ------------- App Entry -------------
fn main() {
    let kill_switch = Arc::new(Mutex::new(false));
    let (_tx, rx) = unbounded::<SimulationFrameMsg>();

    App::new()
        .insert_resource(ClearColor(Color::rgb(0.05, 0.05, 0.1)))
        .insert_resource(FrameTimer(Timer::from_seconds(0.4, TimerMode::Repeating)))
        .insert_resource(LoadingScreen(false))
        .insert_resource(LoadingTextTimer {
            timer: Timer::from_seconds(0.5, TimerMode::Repeating),
            dot_count: 0,
        })
        .insert_resource(PlaybackControl {
            speed: 0.4,
            ..default()
        })
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
        .insert_resource(SimulationStats::new(1, None))
        .insert_resource(ShowGraphs(false))
        .insert_resource(NdjsonChannel(rx))
        .insert_resource(NdjsonTailingHandle(None))
        .insert_resource(NdjsonKillSwitch(kill_switch.clone()))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "🔥 Forest Fire Simulation 3D".into(),
                resolution: (1280., 800.).into(),
                resizable: false,
                mode: bevy::window::WindowMode::Fullscreen,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin)
        .add_systems(Startup, setup_assets)
        .add_systems(
            Update,
            (
                simulation_update_system,
                ui_system,
                advance_frame_system,
                camera_movement_system,
                space_pause_resume_system,
                start_simulation_button_system,
            ),
        )
        .run();
}
// ─────────────────────────────────────────────────────────────────────────────
// Helper to spawn camera & lights for a fresh simulation run

// ─────────────────────────────────────────────────────────────────────────────
// Start Simulation Button Handler
fn start_simulation_button_system(
    mut params: ResMut<SimulationParams>,
    mut ndjson_handle: ResMut<NdjsonTailingHandle>,
    kill_switch: Res<NdjsonKillSwitch>,
    mut commands: Commands,
    mut playback: ResMut<PlaybackControl>,
    mut loading: ResMut<LoadingScreen>,
    old_entities: Query<Entity, With<SimulationEntity>>,
) {
    if !params.trigger_simulation || loading.0 {
        return;
    }
    params.trigger_simulation = false;

    // 1) despawn previous run
    for e in old_entities.iter() {
        commands.entity(e).despawn_recursive();
    }

    // 2) show loading + lock UI
    loading.0 = true;
    playback.paused = true;
    playback.jump_to_frame = Some(0);

    // 3) kill old NDJSON tailer
    if let Some(handle) = ndjson_handle.0.take() {
        *kill_switch.0.lock().unwrap() = true;
        let _ = handle.join();
        *kill_switch.0.lock().unwrap() = false;
    }

    // 4) remove old stream file
    let _ = std::fs::remove_file("assets/simulation_stream.ndjson");

    // 6) hook up new tailer
    let (_tx, rx) = unbounded::<SimulationFrameMsg>();
    commands.insert_resource(NdjsonChannel(rx));
    let handle = spawn_ndjson_tailer(
        _tx,
        "assets/simulation_stream.ndjson".to_string(),
        kill_switch.0.clone(),
    );
    ndjson_handle.0 = Some(handle);

    // 5) launch backend
    let cmdline = vec![
        params.width.to_string(),
        params.height.to_string(),
        params.burning_trees.to_string(),
        params.burning_grasses.to_string(),
        (params.is_wind_toggled as i32).to_string(),
        params.wind_angle.to_string(),
        params.wind_strength.to_string(),
        params.number_of_steps.to_string(),
    ];
    let full_cmd = format!("sh run-sim-ndjson.sh {}", cmdline.join(" "));
    thread::spawn(move || {
        let _ = Command::new("sh")
            .arg("-c")
            .arg(full_cmd)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    });

    // 7) clear old Simulation & stats
    commands.remove_resource::<Simulation>();
    commands.insert_resource(SimulationStats::new(1, None));

    // 8) unpause backend so it starts emitting frames
    update_sim_control(SimControl {
        paused: Some(false),
        ..Default::default()
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// Simulation NDJSON Receiver System
fn simulation_update_system(
    mut commands: Commands,
    ndjson: Res<NdjsonChannel>,
    mut stats: ResMut<SimulationStats>,
    mut loading: ResMut<LoadingScreen>,
    mut playback: ResMut<PlaybackControl>,
    mut sim: Option<ResMut<Simulation>>,
    mut has_started: Local<bool>,
) {
    while let Ok(msg) = ndjson.0.try_recv() {
        match msg {
            SimulationFrameMsg::Metadata { width, height } => {
                // Initialize Simulation resource
                commands.insert_resource(Simulation {
                    frames: Vec::new(),
                    current: 0,
                    width,
                    height,
                });

                // Reset stats and playback
                commands.insert_resource(SimulationStats::new(1, None));
                playback.paused = true;
                playback.jump_to_frame = Some(0);

                *has_started = true;
            }

            SimulationFrameMsg::Frame(frame) => {
                // If Simulation is available, push frame
                if let Some(ref mut sim) = sim {
                    sim.frames.push(frame.clone());

                    if sim.frames.len() >= 2 && loading.0 {
                        loading.0 = false;
                        playback.paused = false;
                        spawn_scene(&mut commands);
                    }

                    // Update stats
                    let mut trees = 0;
                    let mut burning_trees = 0;
                    let mut tree_ashes = 0;
                    let mut grasses = 0;
                    let mut burning_grasses = 0;
                    let mut grass_ashes = 0;

                    for row in &frame {
                        for cell in row {
                            match cell.as_str() {
                                "T" => trees += 1,
                                "*" | "**" | "***" => burning_trees += 1,
                                "A" => tree_ashes += 1,
                                "G" => grasses += 1,
                                "+" => burning_grasses += 1,
                                "-" => grass_ashes += 1,
                                _ => {}
                            }
                        }
                    }

                    let frame_index = sim.frames.len() - 1;

                    if stats.trees_over_time.len() <= frame_index {
                        stats.trees_over_time.push(trees);
                        stats.burning_trees_over_time.push(burning_trees);
                        stats.tree_ashes_over_time.push(tree_ashes);
                        stats.grasses_over_time.push(grasses);
                        stats.burning_grasses_over_time.push(burning_grasses);
                        stats.grass_ashes_over_time.push(grass_ashes);
                    } else {
                        stats.trees_over_time[frame_index] = trees;
                        stats.burning_trees_over_time[frame_index] = burning_trees;
                        stats.tree_ashes_over_time[frame_index] = tree_ashes;
                        stats.grasses_over_time[frame_index] = grasses;
                        stats.burning_grasses_over_time[frame_index] = burning_grasses;
                        stats.grass_ashes_over_time[frame_index] = grass_ashes;
                    }

                    stats.frame_counter = sim.frames.len();
                } else if *has_started {
                    // Simulation resource was just inserted but system didn't get fresh ResMut yet
                    // Insert manually with first frame
                    let width = frame[0].len();
                    let height = frame.len();
                    commands.insert_resource(Simulation {
                        frames: vec![frame.clone()],
                        current: 0,
                        width,
                        height,
                    });

                    stats.frame_counter = 1;

                    // Update stats for frame 0
                    let mut trees = 0;
                    let mut burning_trees = 0;
                    let mut tree_ashes = 0;
                    let mut grasses = 0;
                    let mut burning_grasses = 0;
                    let mut grass_ashes = 0;

                    for row in &frame {
                        for cell in row {
                            match cell.as_str() {
                                "T" => trees += 1,
                                "*" | "**" | "***" => burning_trees += 1,
                                "A" => tree_ashes += 1,
                                "G" => grasses += 1,
                                "+" => burning_grasses += 1,
                                "-" => grass_ashes += 1,
                                _ => {}
                            }
                        }
                    }

                    stats.trees_over_time = vec![trees];
                    stats.burning_trees_over_time = vec![burning_trees];
                    stats.tree_ashes_over_time = vec![tree_ashes];
                    stats.grasses_over_time = vec![grasses];
                    stats.burning_grasses_over_time = vec![burning_grasses];
                    stats.grass_ashes_over_time = vec![grass_ashes];
                }
            }

            SimulationFrameMsg::SimulationEnded => {
                // Optional: handle graceful shutdown
            }
        }
    }
}

// ------------- Asset setup -------------
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
    mesh_map.insert("burning_leaves1", mesh_map["leaves"].clone());
    mesh_map.insert("burning_leaves2", mesh_map["leaves"].clone());
    mesh_map.insert("burning_leaves3", mesh_map["leaves"].clone());

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
        "burning_leaves1",
        materials.add(StandardMaterial {
            base_color: Color::rgb(1.0, 0.4, 0.2),
            emissive: Color::rgb(3.0, 1.0, 0.5),
            ..default()
        }),
    );
    mat_map.insert(
        "burning_leaves2",
        materials.add(StandardMaterial {
            base_color: Color::rgb(0.6, 0.18, 0.08),
            emissive: Color::rgb(1.5, 0.6, 0.3),
            ..default()
        }),
    );
    mat_map.insert(
        "burning_leaves3",
        materials.add(StandardMaterial {
            base_color: Color::rgb(0.23, 0.07, 0.02),
            emissive: Color::rgb(0.5, 0.12, 0.08),
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

// ------------- Frame advance system -------------
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
    if sim.frames.is_empty() {
        return;
    }
    if timer.0.duration().as_secs_f32() != playback.speed {
        timer
            .0
            .set_duration(Duration::from_secs_f32(playback.speed));
    }
    if playback.paused
        && !playback.step_forward
        && !playback.step_back
        && playback.jump_to_frame.is_none()
    {
        return;
    }
    let ticked = timer.0.tick(time.delta()).just_finished();
    if !ticked && playback.jump_to_frame.is_none() && !playback.step_forward && !playback.step_back
    {
        return;
    }
    let last = sim.frames.len().saturating_sub(1);
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
            next = 0;
            playback.paused = true;
        }
    }
    sim.current = next;
    for ent in cells.iter() {
        commands.entity(ent).despawn_recursive();
    }
    let grid = &sim.frames[sim.current];
    let cell_size = 10.0;
    let spacing = 1.5;
    let offset_x = -(sim.width as f32 * cell_size * spacing) / 2.0;
    let offset_z = -(sim.height as f32 * cell_size * spacing) / 2.0;
    let height = grid.len();
    let width = grid[0].len();
    for (iy, row) in grid.iter().enumerate() {
        let y = height - 1 - iy;
        for (ix, cell) in row.iter().enumerate() {
            let x = width - 1 - ix;
            let pos = Vec3::new(
                offset_x + y as f32 * cell_size * spacing, // <--- y and x swapped!
                0.0,
                offset_z - x as f32 * cell_size * spacing, // <--- and x is now negative in Z
            );

            match cell.as_str() {
                "T" | "*" | "**" | "***" => {
                    spawn_cell(&mut commands, &cache, "trunk", pos + Vec3::Y * 2.0);
                    spawn_cell(
                        &mut commands,
                        &cache,
                        match cell.as_str() {
                            "*" => "burning_leaves1",
                            "**" => "burning_leaves2",
                            "***" => "burning_leaves3",
                            _ => "leaves",
                        },
                        pos + Vec3::Y * 7.0,
                    );
                }
                other => {
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
}

// ------------- Kind from string -------------
fn kind_from_str(cell: &str) -> &'static str {
    match cell {
        "G" => "grass",
        "A" => "ash",
        "W" => "water",
        "+" => "burning_grass",
        "-" => "burnt_grass",
        "*" => "burning_leaves1",
        "**" => "burning_leaves2",
        "***" => "burning_leaves3",
        other => panic!(
            "Unknown cell type encountered in kind_from_str: '{}'",
            other
        ),
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

// ------------- Camera movement -------------
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
    if buttons.pressed(MouseButton::Left) {
        let mut delta = Vec2::ZERO;
        for ev in mouse_motion_events.read() {
            delta += ev.delta;
        }
        if delta.length_squared() > 0.0 {
            let yaw = Quat::from_rotation_y(-delta.x * 0.002);
            let pitch = Quat::from_rotation_x(-delta.y * 0.002);
            transform.rotation = yaw * transform.rotation;
            transform.rotation = transform.rotation * pitch;
        }
    }
}

// ------------- Pause/Resume -------------
fn space_pause_resume_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut playback: ResMut<PlaybackControl>,
) {
    if keys.just_pressed(KeyCode::Space) {
        playback.paused = !playback.paused;
    }
}

fn ui_system(
    mut contexts: EguiContexts,
    mut params: ResMut<SimulationParams>,
    sim: Option<Res<Simulation>>,
    mut playback: ResMut<PlaybackControl>,
    stats: Res<SimulationStats>,
    mut show_graphs_resource: ResMut<ShowGraphs>,
    loading: Res<LoadingScreen>,
    mut text_timer: ResMut<LoadingTextTimer>,
    time: Res<Time>,
) {
    let ctx = contexts.ctx_mut();
    let sim_ref = sim.as_ref().map(|r| &**r);

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

    egui::SidePanel::left("side_panel")
        .resizable(true)
        .min_width(200.0)
        .show(ctx, |ui| {
            ui.heading("Simulation Controls");
            ui.separator();
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
                if ui.button("Update Wind").clicked() {
                    update_sim_control(SimControl {
                        windAngle: Some(params.wind_angle as i32),
                        windStrength: Some(params.wind_strength as i32),
                        windEnabled: Some(params.is_wind_toggled),
                        ..Default::default()
                    });
                }
            }

            if ui.button("Start Simulation").clicked() {
                params.trigger_simulation = true;
            }

            if let Some(sim) = sim_ref {
                ui.separator();
                ui.label("Playback Controls");
                ui.horizontal(|ui| {
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
                        update_sim_control(SimControl {
                            paused: Some(playback.paused),
                            ..Default::default()
                        });
                    }
                    if ui.small_button("⏭").clicked() {
                        playback.step_forward = true;
                    }
                    if ui.small_button("⏭|").clicked() {
                        playback.jump_to_frame = Some(sim.frames.len().saturating_sub(1));
                    }
                });

                ui.add(egui::Slider::new(&mut playback.speed, 0.05..=2.0).text("Speed s/frame"));
                ui.label(format!("Frame: {}/{}", sim.current + 1, sim.frames.len()));

                let mut display_frame = sim.current + 1;
                if ui
                    .add(egui::Slider::new(&mut display_frame, 1..=sim.frames.len()).text("Frame"))
                    .changed()
                {
                    playback.jump_to_frame = Some(display_frame - 1);
                }

                if playback.paused && ui.button("Step One Frame").clicked() {
                    update_sim_control(SimControl {
                        step: Some(true),
                        ..Default::default()
                    });
                }

                ui.separator();
                if ui
                    .button(if show_graphs_resource.0 {
                        "Hide Graphs"
                    } else {
                        "Show Graphs"
                    })
                    .clicked()
                {
                    show_graphs_resource.0 = !show_graphs_resource.0;
                }
            }
        });

    // ─────────── Graphs ────────────────────
    if let Some(sim) = sim_ref {
        let available = stats.trees_over_time.len();
        if available == 0 {
            return;
        }
        let last_index = sim.current.min(available - 1);

        let total_trees_over_time: Vec<f64> = (0..=last_index)
            .map(|i| {
                (stats.trees_over_time[i]
                    + stats.burning_trees_over_time[i]
                    + stats.tree_ashes_over_time[i]) as f64
            })
            .collect();

        let total_grass_over_time: Vec<f64> = (0..=last_index)
            .map(|i| {
                (stats.grasses_over_time[i]
                    + stats.burning_grasses_over_time[i]
                    + stats.grass_ashes_over_time[i]) as f64
            })
            .collect();

        let initial_total = total_trees_over_time[0] + total_grass_over_time[0];

        if show_graphs_resource.0 {
            egui::Window::new("Simulation Graphs")
                .default_width(550.0)
                .default_height(700.0)
                .show(ctx, |ui| {
                    use egui_plot::{Legend, Line, Plot, PlotPoints};

                    macro_rules! plot_percent {
                        ($name:expr, $v:expr, $total:expr) => {{
                            let points: PlotPoints = (0..=last_index)
                                .map(|i| {
                                    let total = $total[i];
                                    let val = if total > 0.0 {
                                        ($v[i] as f64 / total) * 100.0
                                    } else {
                                        0.0
                                    };
                                    [i as f64, val]
                                })
                                .collect();
                            Line::new(points).name($name)
                        }};
                    }

                    ui.label("Tree Status (%)");
                    Plot::new("Trees")
                        .legend(Legend::default())
                        .height(120.0)
                        .show(ui, |plot_ui| {
                            plot_ui.line(plot_percent!(
                                "Trees %",
                                stats.trees_over_time,
                                total_trees_over_time
                            ));
                            plot_ui.line(plot_percent!(
                                "Burning %",
                                stats.burning_trees_over_time,
                                total_trees_over_time
                            ));
                            plot_ui.line(plot_percent!(
                                "Ashes %",
                                stats.tree_ashes_over_time,
                                total_trees_over_time
                            ));
                        });

                    ui.label("Grass Status (%)");
                    Plot::new("Grasses")
                        .legend(Legend::default())
                        .height(120.0)
                        .show(ui, |plot_ui| {
                            plot_ui.line(plot_percent!(
                                "Grass %",
                                stats.grasses_over_time,
                                total_grass_over_time
                            ));
                            plot_ui.line(plot_percent!(
                                "Burning %",
                                stats.burning_grasses_over_time,
                                total_grass_over_time
                            ));
                            plot_ui.line(plot_percent!(
                                "Ashes %",
                                stats.grass_ashes_over_time,
                                total_grass_over_time
                            ));
                        });

                    ui.label("Burning Cells (%)");
                    Plot::new("Burning")
                        .legend(Legend::default())
                        .height(120.0)
                        .show(ui, |plot_ui| {
                            let points: PlotPoints = (0..=last_index)
                                .map(|i| {
                                    let val = stats.burning_grasses_over_time[i]
                                        + stats.burning_trees_over_time[i];
                                    let pct = if initial_total > 0.0 {
                                        (val as f64 / initial_total) * 100.0
                                    } else {
                                        0.0
                                    };
                                    [i as f64, pct]
                                })
                                .collect();
                            plot_ui.line(Line::new(points).name("Burning %"));
                        });

                    ui.label("New Burning Per Step");
                    Plot::new("NewBurning")
                        .legend(Legend::default())
                        .height(120.0)
                        .show(ui, |plot_ui| {
                            let mut prev = 0;
                            let points: PlotPoints = (0..=last_index)
                                .map(|i| {
                                    let now = stats.burning_grasses_over_time[i]
                                        + stats.burning_trees_over_time[i];
                                    let diff = if i == 0 {
                                        now
                                    } else {
                                        now.saturating_sub(prev)
                                    };
                                    prev = now;
                                    [i as f64, diff as f64]
                                })
                                .collect();
                            plot_ui.line(Line::new(points).name("New Burning"));
                        });

                    ui.label("Burned Area (%)");
                    Plot::new("BurnedArea")
                        .legend(Legend::default())
                        .height(120.0)
                        .show(ui, |plot_ui| {
                            let points: PlotPoints = (0..=last_index)
                                .map(|i| {
                                    let burned = stats.tree_ashes_over_time[i]
                                        + stats.grass_ashes_over_time[i];
                                    let pct = if initial_total > 0.0 {
                                        (burned as f64 / initial_total) * 100.0
                                    } else {
                                        0.0
                                    };
                                    [i as f64, pct]
                                })
                                .collect();
                            plot_ui.line(Line::new(points).name("% Burned"));
                        });
                });
        }
    }
}

// At the bottom or top of your file
fn spawn_scene(commands: &mut Commands) {
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
                illuminance: 10_000.0,
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
                intensity: 5_000.0,
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
