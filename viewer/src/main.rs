use bevy::math::primitives::{Cuboid, Cylinder, Sphere};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPlugin};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

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
        .insert_resource(PlaybackControl::default())
        .insert_resource(LoadingScreen(false))
        .insert_resource(PendingSimulation::default())
        .insert_resource(LoadingTextTimer {
            timer: Timer::from_seconds(0.5, TimerMode::Repeating),
            dot_count: 0,
        })
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
        .add_systems(Startup, setup_cached_assets)
        .add_systems(Update, ui_system)
        .add_systems(Update, start_simulation_button_system)
        .add_systems(Update, check_simulation_ready_system)
        .add_systems(Update, advance_frame)
        .run();
}

fn setup_cached_assets(
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
    mesh_map.insert("fire", meshes.add(Mesh::from(Cylinder::new(5.0, 0.5))));
    mesh_map.insert("burnt", meshes.add(Mesh::from(Cylinder::new(5.0, 0.2))));

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
        "fire",
        materials.add(StandardMaterial {
            base_color: Color::rgb(0.9, 0.2, 0.0),
            emissive: Color::rgb(3.0, 1.2, 0.3),
            ..default()
        }),
    );
    mat_map.insert(
        "burnt",
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

fn start_simulation_button_system(
    mut commands: Commands,
    mut sim_params: ResMut<SimulationParams>,
    mut loading: ResMut<LoadingScreen>,
    mut pending: ResMut<PendingSimulation>,
    q_old_entities: Query<Entity, With<SimulationEntity>>,
) {
    if sim_params.trigger_simulation && !loading.0 {
        for entity in q_old_entities.iter() {
            if commands.get_entity(entity).is_some() {
                commands.entity(entity).despawn_recursive();
            }
        }

        loading.0 = true;
        let result = Arc::new(Mutex::new(None));
        let result_clone = Arc::clone(&result);
        let progress = Arc::new(Mutex::new(0.0));
        let progress_clone = Arc::clone(&progress);

        // ✅ Run precompiled Scala instead of sbt

        let script_path = std::fs::canonicalize("../run-sim.sh").expect("Script not found");
        let command = format!(
            "{} {} {} {} {} {}",
            script_path.display(),
            sim_params.width,
            sim_params.height,
            sim_params.burning_trees,
            sim_params.burning_grasses,
            sim_params.number_of_steps
        );

        let handle = thread::spawn(move || {
            let mut child = Command::new("sh")
                .arg("-c")
                .arg(&command)
                .current_dir("../")
                .stdout(Stdio::piped())
                .spawn()
                .expect("Failed to start Scala process");

            if let Some(stdout) = child.stdout.take() {
                let reader = BufReader::new(stdout);
                for line in reader.lines().flatten() {
                    if let Some(p) = line.strip_prefix("PROGRESS:") {
                        if let Ok(val) = p.trim().parse::<f32>() {
                            *progress_clone.lock().unwrap() = val.min(100.0);
                        }
                    } else {
                        println!("{line}");
                    }
                }
            }

            if let Ok(status) = child.wait() {
                if status.success() {
                    // Scala finished successfully, now load the JSON
                    if let Some(data) = load_simulation_data() {
                        *result_clone.lock().unwrap() = Some(data);
                    } else {
                        eprintln!(
                            "⚠️ Simulation JSON file not found or invalid after Scala finished."
                        );
                    }
                } else {
                    eprintln!("⚠️ Scala simulation failed to exit successfully.");
                }
            } else {
                eprintln!("⚠️ Could not wait on Scala process.");
            }
        });

        pending.handle = Some(handle);
        pending.result = result;
        pending.progress = progress;
        sim_params.trigger_simulation = false;
    }
}

fn check_simulation_ready_system(
    mut commands: Commands,
    mut loading: ResMut<LoadingScreen>,
    mut pending: ResMut<PendingSimulation>,
) {
    if !loading.0 {
        return;
    }

    let data_opt = {
        let guard = pending.result.lock().unwrap();
        guard.clone()
    };

    if let Some(data) = data_opt {
        commands.insert_resource(Simulation {
            frames: data.steps,
            current: 0,
            width: data.width,
            height: data.height,
        });

        commands.spawn((
            Camera3dBundle {
                transform: Transform::from_xyz(0.0, 250.0, 400.0).looking_at(Vec3::ZERO, Vec3::Y),
                ..default()
            },
            MainCamera,
            SimulationEntity,
        ));

        commands.spawn((
            DirectionalLightBundle {
                directional_light: DirectionalLight {
                    shadows_enabled: true,
                    illuminance: 10000.0,
                    ..default()
                },
                transform: Transform::from_xyz(0.0, 200.0, 100.0).looking_at(Vec3::ZERO, Vec3::Y),
                ..default()
            },
            SimulationEntity,
        ));

        commands.spawn((
            PointLightBundle {
                point_light: PointLight {
                    intensity: 5000.0,
                    range: 500.0,
                    shadows_enabled: true,
                    ..default()
                },
                transform: Transform::from_xyz(100.0, 150.0, 100.0),
                ..default()
            },
            SimulationEntity,
        ));

        commands.insert_resource(AmbientLight {
            color: Color::WHITE,
            brightness: 0.2,
        });

        loading.0 = false;
        *pending = PendingSimulation::default();
    }
}

fn advance_frame(
    mut commands: Commands,
    time: Res<Time>,
    mut timer: ResMut<FrameTimer>,
    mut sim: Option<ResMut<Simulation>>,
    mut playback: ResMut<PlaybackControl>,
    q_cells: Query<Entity, With<CellEntity>>,
    cache: Res<CachedAssets>,
) {
    if playback.paused
        && !playback.step_forward
        && !playback.step_back
        && !playback.go_to_start
        && !playback.go_to_end
    {
        return;
    }
    if !timer.0.tick(time.delta()).just_finished()
        && !playback.step_forward
        && !playback.step_back
        && !playback.go_to_start
        && !playback.go_to_end
    {
        return;
    }
    let Some(mut sim) = sim else {
        return;
    };
    for entity in q_cells.iter() {
        commands.entity(entity).despawn_recursive();
    }
    if playback.go_to_start {
        sim.current = 0;
        playback.go_to_start = false;
    } else if playback.go_to_end {
        sim.current = sim.frames.len() - 1;
        playback.go_to_end = false;
    } else if playback.step_back {
        if sim.current > 0 {
            sim.current -= 1;
        }
        playback.step_back = false;
    }
    let grid = &sim.frames[sim.current];
    let cell_size = 10.0;
    let spacing = 1.5;
    let offset_x = -(sim.width as f32 * cell_size * spacing) / 2.0;
    let offset_z = -(sim.height as f32 * cell_size * spacing) / 2.0;
    for (y, row) in grid.iter().enumerate() {
        for (x, cell) in row.iter().enumerate() {
            let pos = Vec3::new(
                offset_x + x as f32 * cell_size * spacing,
                0.0,
                offset_z + y as f32 * cell_size * spacing,
            );
            match cell.as_str() {
                "T" | "*" => {
                    let burning = cell == "*";
                    commands.spawn((
                        PbrBundle {
                            mesh: cache.meshes["trunk"].clone(),
                            material: cache.materials["trunk"].clone(),
                            transform: Transform::from_translation(pos + Vec3::Y * 2.0),
                            ..default()
                        },
                        CellEntity,
                        SimulationEntity,
                    ));
                    commands.spawn((
                        PbrBundle {
                            mesh: cache.meshes["leaves"].clone(),
                            material: cache.materials
                                [if burning { "burning_leaves" } else { "leaves" }]
                            .clone(),
                            transform: Transform::from_translation(pos + Vec3::Y * 7.0),
                            ..default()
                        },
                        CellEntity,
                        SimulationEntity,
                    ));
                }
                "A" => spawn_cell(&mut commands, &cache, "ash", pos),
                "G" => spawn_cell(&mut commands, &cache, "grass", pos),
                "W" => spawn_cell(&mut commands, &cache, "water", pos),
                "+" => spawn_cell(&mut commands, &cache, "fire", pos),
                "-" => spawn_cell(&mut commands, &cache, "burnt", pos),
                _ => spawn_cell(&mut commands, &cache, "ash", pos),
            }
        }
    }
    if playback.step_forward {
        sim.current = (sim.current + 1).min(sim.frames.len() - 1);
        playback.step_forward = false;
    } else if !playback.paused {
        sim.current = (sim.current + 1) % sim.frames.len();
    }
}
fn spawn_cell(commands: &mut Commands, cache: &CachedAssets, kind: &str, pos: Vec3) {
    commands.spawn((
        PbrBundle {
            mesh: cache.meshes[kind].clone(),
            material: cache.materials[kind].clone(),
            transform: Transform::from_translation(pos + Vec3::Y * 0.25),
            ..default()
        },
        CellEntity,
        SimulationEntity,
    ));
}

fn ui_system(
    mut contexts: EguiContexts,
    mut params: ResMut<SimulationParams>,
    sim: Option<Res<Simulation>>,
    loading: Res<LoadingScreen>,
    mut text_timer: ResMut<LoadingTextTimer>,
    time: Res<Time>,
    mut playback: ResMut<PlaybackControl>,
) {
    if loading.0 {
        text_timer.timer.tick(time.delta());
        if text_timer.timer.just_finished() {
            text_timer.dot_count = (text_timer.dot_count + 1) % 4;
        }
        let dots = ".".repeat(text_timer.dot_count);
        egui::TopBottomPanel::top("loading_panel").show(contexts.ctx_mut(), |ui| {
            ui.vertical_centered(|ui| {
                ui.heading("⏳ Generating Simulation");
                ui.label(format!("Loading{}", dots));
            });
        });
        return;
    }
    egui::Window::new("Simulation Controls").show(contexts.ctx_mut(), |ui| {
        ui.add(egui::Slider::new(&mut params.width, 10..=100).text("Width"));
        ui.add(egui::Slider::new(&mut params.height, 10..=100).text("Height"));
        ui.add(egui::Slider::new(&mut params.burning_trees, 0..=100).text("Burning trees %"));
        ui.add(egui::Slider::new(&mut params.burning_grasses, 0..=100).text("Burning grasses %"));
        ui.add(egui::Slider::new(&mut params.number_of_steps, 1..=100).text("Number of steps"));
        if ui.button("Start Simulation").clicked() {
            params.trigger_simulation = true;
        }
        if sim.is_some() {
            ui.separator();
            ui.label("Playback Controls:");
            ui.horizontal(|ui| {
                if ui.button("|⏮ First").clicked() {
                    playback.go_to_start = true;
                }
                if ui.button("⏮ Go Back").clicked() {
                    playback.step_back = true;
                }
                if ui
                    .button(if playback.paused {
                        "▶ Resume"
                    } else {
                        "⏸ Pause"
                    })
                    .clicked()
                {
                    playback.paused = !playback.paused;
                }
                if ui.button("⏭ Go Forward").clicked() {
                    playback.step_forward = true;
                }
                if ui.button("Last |⏭").clicked() {
                    playback.go_to_end = true;
                }
            });
        }
    });
    if let Some(sim) = sim {
        egui::TopBottomPanel::top("step_panel").show(contexts.ctx_mut(), |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(format!("Step {}/{}", sim.current, sim.frames.len() - 1));
            });
        });
    }
}

fn load_simulation_data() -> Option<GridData> {
    let file = File::open("assets/simulation.json").ok()?;
    let reader = BufReader::new(file);
    serde_json::from_reader(reader).ok()
}
