use bevy::math::primitives::{Cuboid, Cylinder, Sphere};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPlugin};
use serde::Deserialize;
use std::fs::File;
use std::io::BufReader;
use std::process::Command;

#[derive(Deserialize)]
struct GridData {
    width: usize,
    height: usize,
    steps: Vec<Vec<Vec<String>>>,
}

#[derive(Resource, Default)]
struct SimulationParams {
    width: u32,
    height: u32,
    burning_trees: u32,
    burning_grasses: u32,
    trigger_simulation: bool,
}

#[derive(Resource)]
struct Simulation {
    frames: Vec<Vec<Vec<String>>>,
    current: usize,
    width: usize,
    height: usize,
}

#[derive(Component)]
struct CellEntity;

#[derive(Component)]
struct MainCamera;

#[derive(Component)]
struct CellMeta {
    kind: String,
}

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::rgb(0.05, 0.05, 0.1)))
        .insert_resource(SimulationParams {
            width: 20,
            height: 20,
            burning_trees: 5,
            burning_grasses: 5,
            trigger_simulation: false,
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
        .add_systems(Update, ui_system)
        .add_systems(Update, start_simulation_button_system)
        .add_systems(Update, advance_frame)
        .run();
}

fn ui_system(mut contexts: EguiContexts, mut params: ResMut<SimulationParams>) {
    let num_cells = params.width * params.height;

    egui::Window::new("Simulation Controls").show(contexts.ctx_mut(), |ui| {
        ui.add(egui::Slider::new(&mut params.width, 10..=100).text("Width"));
        ui.add(egui::Slider::new(&mut params.height, 10..=100).text("Height"));
        ui.add(egui::Slider::new(&mut params.burning_trees, 1..=num_cells).text("Burning Trees"));
        ui.add(
            egui::Slider::new(&mut params.burning_grasses, 1..=num_cells).text("Burning Grasses"),
        );

        if ui.button("Start Simulation").clicked() {
            params.trigger_simulation = true;
        }
    });
}

fn start_simulation_button_system(
    mut commands: Commands,
    mut sim_params: ResMut<SimulationParams>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
    q_camera: Query<Entity, With<MainCamera>>,
) {
    if sim_params.trigger_simulation {
        for entity in q_camera.iter() {
            commands.entity(entity).despawn_recursive();
        }

        run_scala_simulation(
            sim_params.width,
            sim_params.height,
            sim_params.burning_trees,
            sim_params.burning_grasses,
        );

        if let Some(data) = load_simulation_data() {
            commands.insert_resource(Simulation {
                frames: data.steps,
                current: 0,
                width: data.width,
                height: data.height,
            });

            commands.spawn((
                Camera3dBundle {
                    transform: Transform::from_xyz(0.0, 250.0, 400.0)
                        .looking_at(Vec3::ZERO, Vec3::Y),
                    ..default()
                },
                MainCamera,
            ));

            commands.spawn(DirectionalLightBundle {
                directional_light: DirectionalLight {
                    shadows_enabled: true,
                    illuminance: 10000.0,
                    ..default()
                },
                transform: Transform::from_xyz(0.0, 200.0, 100.0).looking_at(Vec3::ZERO, Vec3::Y),
                ..default()
            });

            commands.spawn(PointLightBundle {
                point_light: PointLight {
                    intensity: 5000.0,
                    range: 500.0,
                    shadows_enabled: true,
                    ..default()
                },
                transform: Transform::from_xyz(100.0, 150.0, 100.0),
                ..default()
            });

            commands.insert_resource(AmbientLight {
                color: Color::WHITE,
                brightness: 0.2,
            });
        }

        sim_params.trigger_simulation = false;
    }
}

fn run_scala_simulation(width: u32, height: u32, burning_trees: u32, burning_grasses: u32) {
    let scala_project_path = "../";

    let command = format!(
        "sbt \"run {} {} {} {}\"",
        width, height, burning_trees, burning_grasses
    );

    let output = Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(scala_project_path)
        .output()
        .expect("Failed to execute Scala simulation");

    if !output.status.success() {
        eprintln!(
            "Simulation generation failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    } else {
        println!("{}", String::from_utf8_lossy(&output.stdout));
    }
}

fn load_simulation_data() -> Option<GridData> {
    let file = File::open("assets/simulation.json").ok()?;
    let reader = BufReader::new(file);
    serde_json::from_reader(reader).ok()
}

fn advance_frame(
    mut commands: Commands,
    time: Res<Time>,
    sim: Option<ResMut<Simulation>>,
    q_cells: Query<Entity, With<CellEntity>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(mut sim) = sim else {
        return;
    };

    static mut TIMER: f32 = 0.0;
    unsafe {
        TIMER += time.delta_seconds();
        if TIMER < 0.4 {
            return;
        }
        TIMER = 0.0;
    }

    for entity in q_cells.iter() {
        commands.entity(entity).despawn_recursive();
    }

    let cell_size = 10.0;
    let spacing = 1.5;
    let offset_x = -(sim.width as f32 * cell_size * spacing) / 2.0;
    let offset_z = -(sim.height as f32 * cell_size * spacing) / 2.0;

    let grid = &sim.frames[sim.current];

    for (y, row) in grid.iter().enumerate() {
        for (x, cell) in row.iter().enumerate() {
            let pos = Vec3::new(
                offset_x + x as f32 * cell_size * spacing,
                0.0,
                offset_z + y as f32 * cell_size * spacing,
            );

            match cell.as_str() {
                "T" | "*" => {
                    let is_burning = cell == "*";
                    let trunk = meshes.add(Mesh::from(Cylinder::new(1.0, 4.0)));
                    let leaves = meshes.add(Mesh::from(Sphere::new(4.0)));

                    let leaf_color = if is_burning {
                        Color::rgb(0.8, 0.1, 0.0)
                    } else {
                        Color::rgb(0.1, 0.6, 0.1)
                    };

                    let leaf_emissive = if is_burning {
                        Color::rgb(3.0, 0.6, 0.3) // simulate brightness
                    } else {
                        Color::BLACK
                    };

                    commands
                        .spawn(PbrBundle {
                            mesh: trunk.clone(),
                            material: materials.add(StandardMaterial {
                                base_color: Color::rgb(0.4, 0.25, 0.1),
                                perceptual_roughness: 1.0,
                                ..default()
                            }),
                            transform: Transform::from_translation(pos + Vec3::Y * 2.0),
                            ..default()
                        })
                        .insert(CellEntity);

                    commands
                        .spawn(PbrBundle {
                            mesh: leaves.clone(),
                            material: materials.add(StandardMaterial {
                                base_color: leaf_color,
                                emissive: leaf_emissive,
                                perceptual_roughness: 0.7,
                                ..default()
                            }),
                            transform: Transform::from_translation(pos + Vec3::Y * 7.0),
                            ..default()
                        })
                        .insert(CellEntity);
                }
                "A" => {
                    let mesh = meshes.add(Mesh::from(Cuboid::new(cell_size, 0.5, cell_size)));
                    let material = materials.add(StandardMaterial {
                        base_color: Color::rgb(0.2, 0.2, 0.2),
                        perceptual_roughness: 1.0,
                        ..default()
                    });
                    commands
                        .spawn(PbrBundle {
                            mesh,
                            material,
                            transform: Transform::from_translation(pos + Vec3::Y * 0.25),
                            ..default()
                        })
                        .insert(CellEntity);
                }
                "G" => {
                    let mesh = meshes.add(Mesh::from(Cylinder::new(5.0, 0.5)));
                    let material = materials.add(StandardMaterial {
                        base_color: Color::rgb(0.1, 0.8, 0.1),
                        ..default()
                    });
                    commands
                        .spawn(PbrBundle {
                            mesh,
                            material,
                            transform: Transform::from_translation(pos + Vec3::Y * 0.25),
                            ..default()
                        })
                        .insert(CellEntity);
                }
                "W" => {
                    let mesh = meshes.add(Mesh::from(Cuboid::new(cell_size, 0.8, cell_size)));
                    let material = materials.add(StandardMaterial {
                        base_color: Color::rgb(0.1, 0.3, 0.8),
                        reflectance: 0.8,
                        perceptual_roughness: 0.3,
                        ..default()
                    });
                    commands
                        .spawn(PbrBundle {
                            mesh,
                            material,
                            transform: Transform::from_translation(pos + Vec3::Y * 0.4),
                            ..default()
                        })
                        .insert(CellEntity);
                }
                "+" => {
                    let mesh = meshes.add(Mesh::from(Cylinder::new(5.0, 0.5)));
                    let material = materials.add(StandardMaterial {
                        base_color: Color::rgb(0.9, 0.2, 0.0),
                        emissive: Color::rgb(3.0, 1.2, 0.3), // simulate brightness
                        perceptual_roughness: 0.4,
                        ..default()
                    });
                    commands
                        .spawn(PbrBundle {
                            mesh,
                            material,
                            transform: Transform::from_translation(pos + Vec3::Y * 0.25),
                            ..default()
                        })
                        .insert(CellEntity);
                }
                "-" => {
                    let mesh = meshes.add(Mesh::from(Cylinder::new(5.0, 0.2)));
                    let material = materials.add(StandardMaterial {
                        base_color: Color::rgb(0.3, 0.3, 0.3),
                        ..default()
                    });
                    commands
                        .spawn(PbrBundle {
                            mesh,
                            material,
                            transform: Transform::from_translation(pos + Vec3::Y * 0.1),
                            ..default()
                        })
                        .insert(CellEntity);
                }
                _ => {
                    let mesh = meshes.add(Mesh::from(Cuboid::new(cell_size, 0.5, cell_size)));
                    let material = materials.add(StandardMaterial {
                        base_color: Color::GRAY,
                        ..default()
                    });
                    commands
                        .spawn(PbrBundle {
                            mesh,
                            material,
                            transform: Transform::from_translation(pos + Vec3::Y * 0.25),
                            ..default()
                        })
                        .insert(CellEntity);
                }
            }
        }
    }

    sim.current = (sim.current + 1) % sim.frames.len();
}
