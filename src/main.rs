use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::input::ButtonInput;
use bevy::math::primitives::{Cuboid, Cylinder, Sphere};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPlugin};
use crossbeam_channel::{unbounded, Receiver, Sender};
use egui_plot::{Legend, Line, Plot, PlotPoints};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{path::Path, sync::mpsc::channel, thread};

use sysinfo::{ProcessRefreshKind, RefreshKind, Signal, System};

//──────────────────── Resources & Data Types ──────────────────────//

fn kill_simulation_processes() {
    let mut sys = System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::everything()),
    );
    sys.refresh_processes();

    let mut killed_any = false;
    for process in sys.processes().values() {
        let cmdline = process.cmd().join(" ");
        let exe = process
            .exe()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let matches = (exe.contains("java") || cmdline.contains("java"))
            && (cmdline.contains("data-generation") && cmdline.contains(".jar"));
        if matches {
            println!("Killing process {}: {}", process.pid(), cmdline);
            let _ = process.kill_with(Signal::Kill);
            killed_any = true;
        }
    }
    if killed_any {
        println!("All simulation processes killed.");
    }
}

// Kills on drop for normal exit
struct KillOnDrop;
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        eprintln!("Exiting (Drop): Killing simulation processes...");
        kill_simulation_processes();
    }
}

#[derive(Resource)]
struct FsWatcher(pub notify::RecommendedWatcher);

enum SimulationFrameMsg {
    Metadata { width: usize, height: usize },
    Frame(Vec<Vec<String>>),
    SimulationEnded,
}

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
    thunder_percentage: u32,
    is_wind_toggled: bool,
    wind_angle: u32,
    wind_strength: u32,
    trigger_simulation: bool,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct SimControl {
    #[serde(rename = "thunderPercentage")]
    pub thunder_percentage: Option<u32>,
    #[serde(rename = "windAngle")]
    pub wind_angle: Option<i32>,
    #[serde(rename = "windStrength")]
    pub wind_strength: Option<i32>,
    #[serde(rename = "windEnabled")]
    pub wind_enabled: Option<bool>,
    pub paused: Option<bool>,
    pub step: Option<bool>,
}

const CONTROL_PATH: &str = "res/sim_control.json";

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
    saplings_over_time: Vec<i64>,
    burning_saplings_over_time: Vec<i64>,
    young_trees_over_time: Vec<i64>,
    burning_young_trees_over_time: Vec<i64>,
    thunder_over_time: Vec<i64>,
}
impl SimulationStats {
    fn new_empty() -> Self {
        Self {
            frame_counter: 0,
            trees_over_time: vec![],
            burning_trees_over_time: vec![],
            tree_ashes_over_time: vec![],
            grasses_over_time: vec![],
            burning_grasses_over_time: vec![],
            grass_ashes_over_time: vec![],
            saplings_over_time: vec![],
            burning_saplings_over_time: vec![],
            young_trees_over_time: vec![],
            burning_young_trees_over_time: vec![],
            thunder_over_time: vec![],
        }
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
#[derive(Resource, Default)]
struct LoadingTextTimer {
    timer: Timer,
    dot_count: usize,
}
#[derive(Resource)]
struct LoadingScreen(pub bool);

#[derive(Deserialize)]
struct FrameMeta {
    width: usize,
    height: usize,
}

//──────────────────── File Helpers ──────────────────────//

fn read_sim_control() -> SimControl {
    if let Ok(content) = fs::read_to_string(CONTROL_PATH) {
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        SimControl::default()
    }
}
fn update_sim_control(update: SimControl) {
    let mut control = read_sim_control();
    if let Some(val) = update.thunder_percentage {
        control.thunder_percentage = Some(val);
    }
    if let Some(val) = update.wind_angle {
        control.wind_angle = Some(val);
    }
    if let Some(val) = update.wind_strength {
        control.wind_strength = Some(val);
    }
    if let Some(val) = update.wind_enabled {
        control.wind_enabled = Some(val);
    }
    if let Some(val) = update.paused {
        control.paused = Some(val);
    }
    control.step = update.step.or(Some(false));
    let json = serde_json::to_string_pretty(&control).unwrap();
    fs::write(CONTROL_PATH, json).expect("Failed to write sim_control.json");
}

//──────────────────── NDJSON File Watcher ──────────────────────//

fn spawn_ndjson_tailer(
    tx: Sender<SimulationFrameMsg>,
    path: &str,
) -> notify::Result<RecommendedWatcher> {
    use std::fs::File;
    use std::io::{BufRead, BufReader, Seek, SeekFrom};

    let parent = Path::new(path).parent().expect("res directory must exist");
    let (tx_fs, rx_fs) = channel::<notify::Result<Event>>();
    let mut watcher = RecommendedWatcher::new(
        move |res| {
            let _ = tx_fs.send(res);
        },
        Config::default(),
    )?;
    watcher.watch(parent, RecursiveMode::NonRecursive)?;

    let path_buf = Path::new(path).to_path_buf();
    thread::spawn(move || {
        // Wait until file exists
        let file = loop {
            match File::open(&path_buf) {
                Ok(f) => break f,
                Err(_) => thread::sleep(Duration::from_millis(50)),
            }
        };
        let mut reader = BufReader::new(file.try_clone().unwrap());
        let mut position = 0u64;
        let mut line = String::new();

        // --- Read metadata ---
        let meta = loop {
            line.clear();
            if reader.read_line(&mut line).unwrap() > 0 {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                position += line.len() as u64;
                if let Ok(m) = serde_json::from_str::<FrameMeta>(trimmed) {
                    break m;
                } else {
                    eprintln!("spawn_ndjson_tailer: first line not metadata, retrying");
                }
            }
            thread::sleep(Duration::from_millis(10));
        };
        let _ = tx.send(SimulationFrameMsg::Metadata {
            width: meta.width,
            height: meta.height,
        });

        // --- Read ALL existing frames ---
        loop {
            line.clear();
            let n = reader.read_line(&mut line).unwrap();
            if n == 0 {
                break; // EOF
            }
            position += n as u64;
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                if let Ok(frame) = serde_json::from_str::<Vec<Vec<String>>>(trimmed) {
                    let _ = tx.send(SimulationFrameMsg::Frame(frame));
                } else {
                    eprintln!("spawn_ndjson_tailer: bad frame, skipping: {trimmed}");
                }
            }
        }

        // --- Tail further frames ---
        while let Ok(res_event) = rx_fs.recv() {
            if let Ok(event) = res_event {
                if matches!(event.kind, EventKind::Modify(_)) {
                    let _ = reader.seek(SeekFrom::Start(position));
                    while let Ok(n) = reader.read_line(&mut line) {
                        if n == 0 {
                            break;
                        }
                        position += n as u64;
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            if let Ok(frame) = serde_json::from_str::<Vec<Vec<String>>>(trimmed) {
                                let _ = tx.send(SimulationFrameMsg::Frame(frame));
                            } else {
                                eprintln!("spawn_ndjson_tailer: bad frame, skipping: {trimmed}");
                            }
                        }
                        line.clear();
                    }
                }
            }
        }
        let _ = tx.send(SimulationFrameMsg::SimulationEnded);
    });

    Ok(watcher)
}

//──────────────────── Asset Setup ──────────────────────//

fn setup_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut mesh_map = HashMap::new();
    // Tree parts
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

    // Burning tree leaves stages
    mesh_map.insert("burning_leaves1", mesh_map["leaves"].clone());
    mesh_map.insert("burning_leaves2", mesh_map["leaves"].clone());
    mesh_map.insert("burning_leaves3", mesh_map["leaves"].clone());

    mesh_map.insert("sapling", meshes.add(Mesh::from(Cylinder::new(0.5, 1.5))));
    mesh_map.insert(
        "young_tree",
        meshes.add(Mesh::from(Cylinder::new(0.8, 3.0))),
    );
    mesh_map.insert("burning_sapling", mesh_map["sapling"].clone());
    mesh_map.insert("burning_young_tree1", mesh_map["young_tree"].clone());
    mesh_map.insert("burning_young_tree2", mesh_map["young_tree"].clone());

    mesh_map.insert("foliage_small", meshes.add(Mesh::from(Sphere::new(1.0))));
    mesh_map.insert("foliage_medium", meshes.add(Mesh::from(Sphere::new(2.5))));
    mesh_map.insert("burning_foliage_small", mesh_map["foliage_small"].clone());
    mesh_map.insert("burning_foliage_medium", mesh_map["foliage_medium"].clone());
    mesh_map.insert(
        "burning_foliage_medium_2",
        mesh_map["foliage_medium"].clone(),
    );

    // Cloud and thunder visual
    mesh_map.insert("cloud", meshes.add(Mesh::from(Sphere::new(4.0))));
    mesh_map.insert("cloud_puff", meshes.add(Mesh::from(Sphere::new(1.0))));
    mesh_map.insert(
        "lightning_segment",
        meshes.add(Mesh::from(Cylinder::new(0.3, 1.0))),
    );

    // Material setup
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
    mat_map.insert(
        "foliage_small",
        materials.add(StandardMaterial {
            base_color: Color::rgb(0.3, 0.7, 0.3),
            ..default()
        }),
    );
    mat_map.insert(
        "foliage_medium",
        materials.add(StandardMaterial {
            base_color: Color::rgb(0.1, 0.6, 0.1),
            ..default()
        }),
    );
    mat_map.insert(
        "burning_foliage_small",
        materials.add(StandardMaterial {
            base_color: Color::rgb(1.0, 0.55, 0.2),
            emissive: Color::rgb(3.0, 1.5, 0.7),
            ..default()
        }),
    );
    mat_map.insert(
        "burning_foliage_medium",
        materials.add(StandardMaterial {
            base_color: Color::rgb(1.0, 0.45, 0.15),
            emissive: Color::rgb(2.6, 1.0, 0.3),
            ..default()
        }),
    );
    mat_map.insert(
        "burning_foliage_medium_2",
        materials.add(StandardMaterial {
            base_color: Color::rgb(0.6, 0.18, 0.08),
            emissive: Color::rgb(1.5, 0.6, 0.3),
            ..default()
        }),
    );
    mat_map.insert(
        "cloud",
        materials.add(StandardMaterial {
            base_color: Color::rgb(0.75, 0.75, 0.8),
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 0.9,
            reflectance: 0.05,
            ..default()
        }),
    );
    mat_map.insert(
        "thunder",
        materials.add(StandardMaterial {
            base_color: Color::rgb(1.0, 1.0, 0.5),
            emissive: Color::rgb(6.0, 6.0, 1.8),
            perceptual_roughness: 0.2,
            reflectance: 0.3,
            ..default()
        }),
    );

    commands.insert_resource(CachedAssets {
        meshes: mesh_map,
        materials: mat_map,
    });
}

//──────────────────── Kind/Spawn helpers ──────────────────────//

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
        "s" => "sapling",
        "y" => "young_tree",
        "!" => "burning_sapling",
        "&" => "burning_young_tree1",
        "@" => "burning_young_tree2",
        "TH" => "thunder",
        other => panic!("Unknown cell type in kind_from_str: '{other}'"),
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

//──────────────────── Scene Spawner ──────────────────────//

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

//──────────────────── Systems: Simulation logic ──────────────────────//

fn start_simulation_button_system(
    mut params: ResMut<SimulationParams>,
    mut commands: Commands,
    mut playback: ResMut<PlaybackControl>,
    mut loading: ResMut<LoadingScreen>,
    old_entities: Query<Entity, With<SimulationEntity>>,
) {
    if !params.trigger_simulation || loading.0 {
        return;
    }
    params.trigger_simulation = false;
    for e in old_entities.iter() {
        commands.entity(e).despawn_recursive();
    }
    loading.0 = true;
    playback.paused = true;
    playback.jump_to_frame = Some(0);

    let _ = std::fs::remove_file("res/simulation_stream.ndjson");
    let cmdline = vec![
        params.width.to_string(),
        params.height.to_string(),
        params.burning_trees.to_string(),
        params.burning_grasses.to_string(),
        params.thunder_percentage.to_string(),
        (params.is_wind_toggled as i32).to_string(),
        params.wind_angle.to_string(),
        params.wind_strength.to_string(),
    ];
    let full_cmd = format!("sh run-sim-ndjson.sh {}", cmdline.join(" "));
    std::thread::spawn(move || {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(full_cmd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();

        match child {
            Ok(mut child_proc) => {
                // stdout
                if let Some(stdout) = child_proc.stdout.take() {
                    std::thread::spawn(move || {
                        let reader = BufReader::new(stdout);
                        for line in reader.lines() {
                            if let Ok(line) = line {
                                eprintln!("scala : print {line}");
                            }
                        }
                    });
                }
                // stderr
                if let Some(stderr) = child_proc.stderr.take() {
                    std::thread::spawn(move || {
                        let reader = BufReader::new(stderr);
                        for line in reader.lines() {
                            if let Ok(line) = line {
                                eprintln!("scala : error {line}");
                            }
                        }
                    });
                }
                let _ = child_proc.wait();
            }
            Err(e) => {
                eprintln!("scala : error (failed to spawn simulation process): {e}");
            }
        }
    });

    let (tx, rx) = unbounded::<SimulationFrameMsg>();
    commands.insert_resource(NdjsonChannel(rx));
    let watcher = spawn_ndjson_tailer(tx, "res/simulation_stream.ndjson")
        .expect("Failed to watch NDJSON file");
    commands.insert_resource(FsWatcher(watcher));

    commands.remove_resource::<Simulation>();
    commands.insert_resource(SimulationStats::new_empty());

    update_sim_control(SimControl {
        paused: Some(false),
        thunder_percentage: Some(params.thunder_percentage),
        wind_enabled: Some(params.is_wind_toggled),
        wind_angle: Some(params.wind_angle as i32),
        wind_strength: Some(params.wind_strength as i32),
        step: Some(false),
    });
}

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
                // Reset stats for a new run!
                *stats = SimulationStats {
                    frame_counter: 0,
                    trees_over_time: vec![],
                    burning_trees_over_time: vec![],
                    tree_ashes_over_time: vec![],
                    grasses_over_time: vec![],
                    burning_grasses_over_time: vec![],
                    grass_ashes_over_time: vec![],
                    saplings_over_time: vec![],
                    burning_saplings_over_time: vec![],
                    young_trees_over_time: vec![],
                    burning_young_trees_over_time: vec![],
                    thunder_over_time: vec![],
                };
                commands.insert_resource(Simulation {
                    frames: Vec::new(),
                    current: 0,
                    width,
                    height,
                });
                playback.paused = true;
                playback.jump_to_frame = Some(0);
                *has_started = true;
            }
            SimulationFrameMsg::Frame(frame) => {
                // Calculate stats for this frame
                let mut trees = 0;
                let mut burning_trees = 0;
                let mut tree_ashes = 0;
                let mut grasses = 0;
                let mut burning_grasses = 0;
                let mut grass_ashes = 0;
                let mut saplings = 0;
                let mut burning_saplings = 0;
                let mut young_trees = 0;
                let mut burning_young_trees = 0;
                let mut thunder = 0; // <--- NEW

                for row in &frame {
                    for cell in row {
                        match cell.as_str() {
                            "T" => trees += 1,
                            "*" | "**" | "***" => burning_trees += 1,
                            "s" => saplings += 1,
                            "!" => burning_saplings += 1,
                            "y" => young_trees += 1,
                            "&" | "@" => burning_young_trees += 1,
                            "A" => tree_ashes += 1,
                            "G" => grasses += 1,
                            "+" => burning_grasses += 1,
                            "-" => grass_ashes += 1,
                            "TH" => thunder += 1, // <-- NEW
                            _ => {}
                        }
                    }
                }
                stats.trees_over_time.push(trees);
                stats.burning_trees_over_time.push(burning_trees);
                stats.tree_ashes_over_time.push(tree_ashes);
                stats.grasses_over_time.push(grasses);
                stats.burning_grasses_over_time.push(burning_grasses);
                stats.grass_ashes_over_time.push(grass_ashes);
                stats.saplings_over_time.push(saplings);
                stats.burning_saplings_over_time.push(burning_saplings);
                stats.young_trees_over_time.push(young_trees);
                stats
                    .burning_young_trees_over_time
                    .push(burning_young_trees);
                stats.thunder_over_time.push(thunder); // <-- NEW

                stats.frame_counter = stats.trees_over_time.len();

                // Insert or update the Simulation resource
                if let Some(ref mut sim) = sim {
                    sim.frames.push(frame.clone());
                } else if *has_started {
                    let width = frame[0].len();
                    let height = frame.len();
                    commands.insert_resource(Simulation {
                        frames: vec![frame.clone()],
                        current: 0,
                        width,
                        height,
                    });
                }

                // Loading logic: leave loading as soon as we have any frames
                if let Some(ref sim) = sim {
                    if sim.frames.len() >= 1 && loading.0 {
                        loading.0 = false;
                        playback.paused = true;
                        playback.jump_to_frame = Some(0);
                        spawn_scene(&mut commands);
                    }
                }
            }
            SimulationFrameMsg::SimulationEnded => {}
        }
    }
}

fn advance_frame_system(
    mut commands: Commands,
    time: Res<Time>,
    mut timer: ResMut<FrameTimer>,
    mut sim: Option<ResMut<Simulation>>,
    mut playback: ResMut<PlaybackControl>,
    cells: Query<Entity, With<CellEntity>>,
    cache: Res<CachedAssets>,
    _stats: ResMut<SimulationStats>,
) {
    let sim = match sim.as_mut() {
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
        if sim.current + 1 < sim.frames.len() {
            next = sim.current + 1;
        }
        playback.step_forward = false;
    } else if !playback.paused && ticked {
        if next < last {
            next += 1;
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
        for (ix, cell) in row.iter().enumerate() {
            let pos = Vec3::new(
                offset_x + (width - 1 - ix) as f32 * cell_size * spacing,
                0.0,
                offset_z + (height - 1 - iy) as f32 * cell_size * spacing,
            );
            match cell.as_str() {
                // Mature tree and burning tree
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
                // Sapling: green and burning (always brown trunk)
                "s" => {
                    commands.spawn((
                        PbrBundle {
                            mesh: cache.meshes["sapling"].clone(),
                            material: cache.materials["trunk"].clone(),
                            transform: Transform::from_translation(pos + Vec3::Y * 0.75),
                            ..default()
                        },
                        CellEntity,
                        SimulationEntity,
                    ));
                    spawn_cell(&mut commands, &cache, "foliage_small", pos + Vec3::Y * 1.6);
                }
                "!" => {
                    commands.spawn((
                        PbrBundle {
                            mesh: cache.meshes["sapling"].clone(),
                            material: cache.materials["trunk"].clone(),
                            transform: Transform::from_translation(pos + Vec3::Y * 0.75),
                            ..default()
                        },
                        CellEntity,
                        SimulationEntity,
                    ));
                    spawn_cell(
                        &mut commands,
                        &cache,
                        "burning_foliage_small",
                        pos + Vec3::Y * 1.6,
                    );
                }
                // Young tree: green and burning stages (always brown trunk)
                "y" => {
                    commands.spawn((
                        PbrBundle {
                            mesh: cache.meshes["young_tree"].clone(),
                            material: cache.materials["trunk"].clone(),
                            transform: Transform::from_translation(pos + Vec3::Y * 1.5),
                            ..default()
                        },
                        CellEntity,
                        SimulationEntity,
                    ));
                    spawn_cell(&mut commands, &cache, "foliage_medium", pos + Vec3::Y * 4.2);
                }
                "&" => {
                    commands.spawn((
                        PbrBundle {
                            mesh: cache.meshes["young_tree"].clone(),
                            material: cache.materials["trunk"].clone(),
                            transform: Transform::from_translation(pos + Vec3::Y * 1.5),
                            ..default()
                        },
                        CellEntity,
                        SimulationEntity,
                    ));
                    spawn_cell(
                        &mut commands,
                        &cache,
                        "burning_foliage_medium",
                        pos + Vec3::Y * 4.2,
                    );
                }
                "@" => {
                    commands.spawn((
                        PbrBundle {
                            mesh: cache.meshes["young_tree"].clone(),
                            material: cache.materials["trunk"].clone(),
                            transform: Transform::from_translation(pos + Vec3::Y * 1.5),
                            ..default()
                        },
                        CellEntity,
                        SimulationEntity,
                    ));
                    spawn_cell(
                        &mut commands,
                        &cache,
                        "burning_foliage_medium_2",
                        pos + Vec3::Y * 4.2,
                    );
                }
                "TH" => {
                    let thunder_parent = commands
                        .spawn((
                            SpatialBundle {
                                transform: Transform::from_translation(pos),
                                ..default()
                            },
                            CellEntity,
                            SimulationEntity,
                        ))
                        .id();

                    commands.entity(thunder_parent).with_children(|parent| {
                        // Tree
                        parent.spawn((PbrBundle {
                            mesh: cache.meshes["trunk"].clone(),
                            material: cache.materials["trunk"].clone(),
                            transform: Transform::from_translation(Vec3::Y * 2.0),
                            ..default()
                        },));
                        parent.spawn((PbrBundle {
                            mesh: cache.meshes["leaves"].clone(),
                            material: cache.materials["leaves"].clone(),
                            transform: Transform::from_translation(Vec3::Y * 7.0),
                            ..default()
                        },));

                        // Cloud puffs
                        let cloud_center = Vec3::Y * (7.0 + 60.0);
                        let puff_offsets = [
                            Vec3::ZERO,
                            Vec3::new(2.0, 0.5, 0.0),
                            Vec3::new(-2.0, 0.5, 0.0),
                            Vec3::new(0.0, 0.5, 2.0),
                            Vec3::new(0.0, 0.5, -2.0),
                            Vec3::new(1.2, 1.0, 1.2),
                            Vec3::new(-1.2, 1.0, -1.2),
                        ];
                        for offset in puff_offsets {
                            parent.spawn((PbrBundle {
                                mesh: cache.meshes["cloud_puff"].clone(),
                                material: cache.materials["cloud"].clone(),
                                transform: Transform::from_translation(cloud_center + offset)
                                    .with_scale(Vec3::splat(2.2)),
                                ..default()
                            },));
                        }

                        // Lightning bolt segments
                        let tree_top = 7.0;
                        let bolt_top = tree_top + 60.0;
                        let segments = 20;
                        let amplitude = 2.5;
                        let radius = 1.2;

                        let mut points = Vec::new();
                        for i in 0..=segments {
                            let t = i as f32 / segments as f32;
                            let y = bolt_top - (bolt_top - tree_top) * t;
                            let x_offset = if i % 2 == 0 { amplitude } else { -amplitude };
                            let z_offset = if i % 4 < 2 { amplitude } else { -amplitude };
                            points.push(Vec3::new(x_offset * (1.0 - t), y, z_offset * (1.0 - t)));
                        }
                        for pair in points.windows(2) {
                            let from = pair[0];
                            let to = pair[1];
                            let dir = to - from;
                            let length = dir.length();
                            let rotation = Quat::from_rotation_arc(Vec3::Y, dir.normalize());
                            let center = from + dir * 0.5;
                            parent.spawn((PbrBundle {
                                mesh: cache.meshes["lightning_segment"].clone(),
                                material: cache.materials["thunder"].clone(),
                                transform: Transform::from_translation(center)
                                    .with_rotation(rotation)
                                    .with_scale(Vec3::new(radius, length / 2.0, radius)),
                                ..default()
                            },));
                        }

                        // Point light for flash
                        parent.spawn((PointLightBundle {
                            transform: Transform::from_translation(cloud_center - Vec3::Y * 1.5),
                            point_light: PointLight {
                                intensity: 18000.0,
                                range: 100.0,
                                shadows_enabled: false,
                                ..default()
                            },
                            ..default()
                        },));
                    });
                }
                // All other types use the kind_from_str mapping
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

//──────────────────── Systems: Camera, Pause, UI ──────────────────────//

fn camera_movement_system(
    mut contexts: EguiContexts,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut mouse_motion_events: EventReader<MouseMotion>,
    mut scroll: EventReader<MouseWheel>,
    mut query: Query<&mut Transform, With<FlyCamera>>,
) {
    let ctx = contexts.ctx_mut();
    if ctx.wants_pointer_input() || ctx.wants_pointer_input() {
        return;
    }
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

fn space_pause_resume_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut playback: ResMut<PlaybackControl>,
) {
    if keys.just_pressed(KeyCode::Space) {
        playback.paused = !playback.paused;
        update_sim_control(SimControl {
            paused: Some(playback.paused),
            ..Default::default()
        });
    }
}

fn handle_plot_click<R>(
    response: &egui_plot::PlotResponse<R>,
    playback: &mut PlaybackControl,
    total_frames: usize,
) {
    if response.response.clicked() {
        if let Some(pos) = response.response.interact_pointer_pos() {
            let plot_coords = response.transform.value_from_position(pos);
            let mut frame = plot_coords.x.round() as isize;
            if frame < 0 {
                frame = 0;
            } else if frame as usize >= total_frames {
                frame = total_frames as isize - 1;
            }
            playback.jump_to_frame = Some(frame as usize);
        }
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

    // Loading screen
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
            ui.add(
                egui::Slider::new(&mut params.thunder_percentage, 0..=100)
                    .text("Thunder per step (%)"),
            );
            ui.add(egui::Checkbox::new(
                &mut params.is_wind_toggled,
                "Enable wind",
            ));
            if params.is_wind_toggled {
                ui.add(egui::Slider::new(&mut params.wind_angle, 0..=359).text("Wind angle °"));
                ui.add(
                    egui::Slider::new(&mut params.wind_strength, 1..=50).text("Wind strength km/h"),
                );
            }
            ui.horizontal(|ui| {
                if ui.button("Start Simulation").clicked() {
                    params.trigger_simulation = true;
                }
                if sim_ref.is_some() && ui.button("Update Thunder").clicked() {
                    update_sim_control(SimControl {
                        thunder_percentage: Some(params.thunder_percentage),
                        ..Default::default()
                    });
                }

                if sim_ref.is_some() && ui.button("Update Wind").clicked() {
                    update_sim_control(SimControl {
                        wind_angle: Some(params.wind_angle as i32),
                        wind_strength: Some(params.wind_strength as i32),
                        wind_enabled: Some(params.is_wind_toggled),
                        ..Default::default()
                    });
                }
            });

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
                        if sim.current + 1 >= sim.frames.len() {
                            update_sim_control(SimControl {
                                step: Some(true),
                                ..Default::default()
                            });
                        }
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

    // Wind indicator
    if params.is_wind_toggled {
        egui::Area::new("wind_indicator")
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-20.0, 20.0))
            .show(ctx, |ui| {
                let desired_size = egui::vec2(120.0, 140.0);
                let (rect, _response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());
                let painter = ui.painter();
                let center = rect.center() - egui::vec2(0.0, 10.0);
                let compass_radius = 50.0;
                painter.circle_stroke(
                    center,
                    compass_radius,
                    egui::Stroke::new(1.0, egui::Color32::GRAY),
                );
                painter.text(
                    center + egui::vec2(0.0, -(compass_radius + 5.0)),
                    egui::Align2::CENTER_CENTER,
                    "N",
                    egui::FontId::proportional(12.0),
                    egui::Color32::LIGHT_GRAY,
                );
                painter.text(
                    center + egui::vec2(0.0, compass_radius + 5.0),
                    egui::Align2::CENTER_CENTER,
                    "S",
                    egui::FontId::proportional(12.0),
                    egui::Color32::LIGHT_GRAY,
                );
                painter.text(
                    center + egui::vec2(compass_radius + 5.0, 0.0),
                    egui::Align2::CENTER_CENTER,
                    "E",
                    egui::FontId::proportional(12.0),
                    egui::Color32::LIGHT_GRAY,
                );
                painter.text(
                    center + egui::vec2(-(compass_radius + 5.0), 0.0),
                    egui::Align2::CENTER_CENTER,
                    "W",
                    egui::FontId::proportional(12.0),
                    egui::Color32::LIGHT_GRAY,
                );
                let wind_goes_to_angle_rad = (params.wind_angle as f32).to_radians();
                let dir_x = wind_goes_to_angle_rad.sin();
                let dir_y = -wind_goes_to_angle_rad.cos();
                let dir = egui::vec2(dir_x, dir_y);
                let max_len = compass_radius - 5.0;
                let min_len = 10.0;
                let strength_ratio = (params.wind_strength.saturating_sub(1)) as f32 / 99.0;
                let length = min_len + strength_ratio * (max_len - min_len);
                let arrow_base = center - dir * length / 2.0;
                let arrow_vec = dir * length;
                painter.arrow(
                    arrow_base,
                    arrow_vec,
                    egui::Stroke::new(3.0, egui::Color32::from_rgb(255, 100, 100)),
                );
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

    // Graphs
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
                .default_height(600.0)
                .resizable(false)
                .show(ctx, |ui| {
                    let screen_height = ctx.screen_rect().height();
                    let available_height = screen_height - 100.0;

                    let num_plots = 6;
                    let label_height = 22.0;
                    let spacing = 8.0;
                    let total_reserved = (label_height + spacing) * num_plots as f32;
                    let plot_height =
                        (available_height - total_reserved).max(40.0) / num_plots as f32;

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
                    let tree_plot = Plot::new("Trees")
                        .legend(Legend::default())
                        .height(plot_height)
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
                    handle_plot_click(&tree_plot, &mut *playback, sim.frames.len());

                    ui.separator();
                    ui.label("Tree Growth Stages (%)");
                    let growth_plot = Plot::new("TreeStages")
                        .legend(Legend::default())
                        .height(plot_height)
                        .show(ui, |plot_ui| {
                            plot_ui.line(plot_percent!(
                                "Saplings %",
                                stats.saplings_over_time,
                                total_trees_over_time
                            ));
                            plot_ui.line(plot_percent!(
                                "Burning Saplings %",
                                stats.burning_saplings_over_time,
                                total_trees_over_time
                            ));
                            plot_ui.line(plot_percent!(
                                "Young Trees %",
                                stats.young_trees_over_time,
                                total_trees_over_time
                            ));
                            plot_ui.line(plot_percent!(
                                "Burning Young Trees %",
                                stats.burning_young_trees_over_time,
                                total_trees_over_time
                            ));
                        });
                    handle_plot_click(&growth_plot, &mut *playback, sim.frames.len());

                    ui.separator();
                    ui.label("Grass Status (%)");
                    let grass_plot = Plot::new("Grasses")
                        .legend(Legend::default())
                        .height(plot_height)
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
                    handle_plot_click(&grass_plot, &mut *playback, sim.frames.len());

                    ui.separator();
                    ui.label("Burning Cells (%)");
                    let burning_plot = Plot::new("Burning")
                        .legend(Legend::default())
                        .height(plot_height)
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
                    handle_plot_click(&burning_plot, &mut *playback, sim.frames.len());

                    ui.separator();
                    ui.label("New Burning Per Step");
                    let new_burning_plot = Plot::new("NewBurning")
                        .legend(Legend::default())
                        .height(plot_height)
                        .show(ui, |plot_ui| {
                            let mut prev_burning = 0;
                            let points: PlotPoints = (0..=last_index)
                                .map(|i| {
                                    let burning_now = stats.burning_grasses_over_time[i]
                                        + stats.burning_trees_over_time[i];
                                    let new_burning = if i == 0 {
                                        burning_now
                                    } else {
                                        burning_now.saturating_sub(prev_burning)
                                    };
                                    prev_burning = burning_now;
                                    [i as f64, new_burning as f64]
                                })
                                .collect();
                            plot_ui.line(Line::new(points).name("New Burning"));

                            // Thunder-caused new burning
                            let thunder_points: PlotPoints = (0..=last_index)
                                .map(|i| {
                                    let thunder_burn = if i == 0 {
                                        0
                                    } else {
                                        stats.thunder_over_time[i - 1]
                                    };
                                    [i as f64, thunder_burn as f64]
                                })
                                .collect();
                            plot_ui.line(Line::new(thunder_points).name("New Burning (Thunder)"));
                        });
                    handle_plot_click(&new_burning_plot, &mut *playback, sim.frames.len());

                    ui.separator();
                    ui.label("Burned Area (%)");
                    let burned_area_plot = Plot::new("BurnedArea")
                        .legend(Legend::default())
                        .height(plot_height)
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
                    handle_plot_click(&burned_area_plot, &mut *playback, sim.frames.len());
                });
        }
    }
}

//──────────────────── App Entrypoint ──────────────────────//

fn main() {
    use bevy::prelude::*;
    use crossbeam_channel::unbounded;
    use std::sync::Mutex;
    // (Insert other necessary use/imports here!)

    // Prevent double cleanup
    let cleaned_up = Arc::new(AtomicBool::new(false));

    // ---- PANIC hook ----
    {
        let cleaned_up = cleaned_up.clone();
        std::panic::set_hook(Box::new(move |info| {
            if !cleaned_up.swap(true, Ordering::SeqCst) {
                eprintln!("PANIC: Killing simulation processes...");
                kill_simulation_processes();
            }
            eprintln!("Process killed. Info: {info}");
        }));
    }

    // ---- SIGINT/Ctrl+C ----
    {
        let cleaned_up = cleaned_up.clone();
        ctrlc::set_handler(move || {
            if !cleaned_up.swap(true, Ordering::SeqCst) {
                eprintln!("SIGINT: Killing simulation processes...");
                kill_simulation_processes();
            }
            std::process::exit(2);
        })
        .expect("Error setting Ctrl+C handler");
    }

    // ---- Drop guard for normal exit ----
    let _guard = KillOnDrop;

    // ---- Your Bevy/Egui App Starts Here ----
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
            ..Default::default()
        })
        .insert_resource(SimulationParams {
            width: 20,
            height: 20,
            burning_trees: 5,
            burning_grasses: 10,
            thunder_percentage: 0,
            is_wind_toggled: false,
            wind_angle: 0,
            wind_strength: 1,
            trigger_simulation: false,
        })
        .insert_resource(SimulationStats::new_empty())
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
                ..Default::default()
            }),
            ..Default::default()
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
                // any other systems you want!
            ),
        )
        .run();
}
