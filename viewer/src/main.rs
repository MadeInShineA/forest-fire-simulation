use bevy::math::primitives::{Cuboid, Cylinder, Sphere};
use bevy::prelude::*;
use serde::Deserialize;
use std::fs::File;
use std::io::BufReader;

#[derive(Deserialize)]
struct GridData {
    width: usize,
    height: usize,
    steps: Vec<Vec<Vec<String>>>,
}

#[derive(Component)]
struct CellEntity;

#[derive(Resource)]
struct Simulation {
    frames: Vec<Vec<Vec<String>>>,
    current: usize,
    width: usize,
    height: usize,
}

#[derive(Component)]
struct MainCamera;

#[derive(Component)]
struct CellMeta {
    kind: String,
}

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::rgb(0.05, 0.05, 0.1)))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "🔥 Forest Fire Simulation 3D".into(),
                resolution: (1280., 800.).into(),
                resizable: false,
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, setup)
        .add_systems(Update, advance_frame)
        .run();
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let file = File::open("assets/simulation.json").expect("File not found");
    let reader = BufReader::new(file);
    let data: GridData = serde_json::from_reader(reader).expect("Invalid JSON");

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

fn advance_frame(
    mut commands: Commands,
    time: Res<Time>,
    mut sim: ResMut<Simulation>,
    q_cells: Query<Entity, With<CellEntity>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
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
            match cell.as_str() {
                "T" | "*" => {
                    let is_burning = cell == "*";

                    let trunk = meshes.add(Mesh::from(Cylinder::new(1.0, 4.0)));
                    let leaves = meshes.add(Mesh::from(Sphere::new(4.0)));

                    let pos = Vec3::new(
                        offset_x + x as f32 * cell_size * spacing,
                        2.0,
                        offset_z + y as f32 * cell_size * spacing,
                    );

                    let leaf_color = if is_burning {
                        Color::rgb(0.8, 0.1, 0.0)
                    } else {
                        Color::rgb(0.1, 0.6, 0.1)
                    };

                    let leaf_emissive = if is_burning {
                        Color::rgb(1.0, 0.3, 0.1)
                    } else {
                        Color::BLACK
                    };

                    // Trunk
                    commands
                        .spawn(PbrBundle {
                            mesh: trunk.clone(),
                            material: materials.add(StandardMaterial {
                                base_color: Color::rgb(0.4, 0.25, 0.1),
                                perceptual_roughness: 1.0,
                                ..default()
                            }),
                            transform: Transform::from_translation(pos),
                            ..default()
                        })
                        .insert(CellEntity);

                    // Leaves
                    commands
                        .spawn(PbrBundle {
                            mesh: leaves.clone(),
                            material: materials.add(StandardMaterial {
                                base_color: leaf_color,
                                emissive: leaf_emissive,
                                perceptual_roughness: 0.7,
                                ..default()
                            }),
                            transform: Transform::from_translation(pos + Vec3::Y * 5.0),
                            ..default()
                        })
                        .insert(CellEntity);

                    continue;
                }
                _ => {}
            }

            let (mesh, base_color, emissive, height) = match cell.as_str() {
                "A" => (
                    meshes.add(Mesh::from(Cuboid::new(cell_size, 1.0, cell_size))),
                    Color::rgb(0.2, 0.2, 0.2),
                    Color::BLACK,
                    1.0,
                ),
                "G" => (
                    meshes.add(Mesh::from(Cylinder::new(5.0, 0.5))),
                    Color::rgb(0.1, 0.8, 0.1),
                    Color::BLACK,
                    0.5,
                ),
                "+" => (
                    meshes.add(Mesh::from(Cylinder::new(5.0, 0.5))),
                    Color::rgb(0.9, 0.2, 0.0),
                    Color::rgb(1.0, 0.5, 0.1),
                    0.5,
                ),
                "-" => (
                    meshes.add(Mesh::from(Cylinder::new(5.0, 0.2))),
                    Color::rgb(0.3, 0.3, 0.3),
                    Color::BLACK,
                    0.2,
                ),
                "W" => (
                    meshes.add(Mesh::from(Cuboid::new(cell_size, 0.8, cell_size))),
                    Color::rgb(0.1, 0.3, 0.8),
                    Color::BLACK,
                    0.8,
                ),
                _ => (
                    meshes.add(Mesh::from(Cuboid::new(cell_size, 0.5, cell_size))),
                    Color::GRAY,
                    Color::BLACK,
                    0.5,
                ),
            };

            let transform = Transform::from_xyz(
                offset_x + x as f32 * cell_size * spacing,
                height / 2.0,
                offset_z + y as f32 * cell_size * spacing,
            );

            commands
                .spawn(PbrBundle {
                    mesh,
                    material: materials.add(StandardMaterial {
                        base_color,
                        emissive,
                        perceptual_roughness: 0.8,
                        ..default()
                    }),
                    transform,
                    ..default()
                })
                .insert(CellEntity)
                .insert(CellMeta { kind: cell.clone() });
        }
    }

    sim.current = (sim.current + 1) % sim.frames.len();
}
