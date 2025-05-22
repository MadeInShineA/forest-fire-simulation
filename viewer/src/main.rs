use bevy::math::primitives::{Cuboid, Cylinder};
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

fn setup(mut commands: Commands) {
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
            transform: Transform::from_xyz(0.0, 200.0, 300.0)
                .looking_at(Vec3::new(0.0, 0.0, 0.0), Vec3::Y),
            ..default()
        },
        MainCamera,
    ));

    commands.spawn(PointLightBundle {
        transform: Transform::from_xyz(200.0, 400.0, 200.0),
        ..default()
    });
}

fn advance_frame(
    mut commands: Commands,
    time: Res<Time>,
    mut sim: ResMut<Simulation>,
    q_cells: Query<(Entity, &CellMeta), With<CellEntity>>,
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

    // Fully despawn all current cells
    for (entity, _) in q_cells.iter() {
        commands.entity(entity).despawn_recursive();
    }

    let cell_size = 10.0;
    let spacing = 1.5;
    let offset_x = -(sim.width as f32 * cell_size * spacing) / 2.0;
    let offset_z = -(sim.height as f32 * cell_size * spacing) / 2.0;

    let grid = &sim.frames[sim.current];

    for (y, row) in grid.iter().enumerate() {
        for (x, cell) in row.iter().enumerate() {
            let (shape, color, height) = match cell.as_str() {
                "T" => (
                    Mesh::from(Cylinder::new(3.0, 12.0)),
                    Color::rgb(0.1, 0.6, 0.1),
                    12.0,
                ), // Tree
                "*" => (
                    Mesh::from(Cylinder::new(3.0, 12.0)),
                    Color::rgb(1.0, 0.0, 0.0),
                    12.0,
                ), // Burning tree
                "A" => (
                    Mesh::from(Cuboid::new(cell_size, 2.0, cell_size)),
                    Color::rgb(0.4, 0.4, 0.4),
                    2.0,
                ), // Tree ash
                "G" => (
                    Mesh::from(Cylinder::new(5.0, 1.0)),
                    Color::rgb(0.2, 1.0, 0.2),
                    1.0,
                ), // Grass
                "+" => (
                    Mesh::from(Cylinder::new(5.0, 1.0)),
                    Color::rgb(1.0, 0.0, 0.0),
                    1.0,
                ), // Burning grass
                "-" => (
                    Mesh::from(Cylinder::new(5.0, 0.5)),
                    Color::rgb(0.4, 0.4, 0.4),
                    0.5,
                ), // Grass ash
                "W" => (
                    Mesh::from(Cuboid::new(cell_size, 5.0, cell_size)),
                    Color::rgb(0.1, 0.3, 0.9),
                    5.0,
                ), // Water
                _ => (
                    Mesh::from(Cuboid::new(cell_size, 1.0, cell_size)),
                    Color::WHITE,
                    1.0,
                ),
            };

            let transform = Transform::from_xyz(
                offset_x + x as f32 * cell_size * spacing,
                height / 2.0,
                offset_z + y as f32 * cell_size * spacing,
            );

            commands
                .spawn(PbrBundle {
                    mesh: meshes.add(shape),
                    material: materials.add(StandardMaterial {
                        base_color: color,
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

fn entity_to_grid(entity: Entity, width: usize) -> (usize, usize) {
    let id = entity.index() as usize;
    let x = id % width;
    let y = id / width;
    (x, y)
}
