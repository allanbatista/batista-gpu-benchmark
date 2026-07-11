//! Benchmark scene: central GLB model (normalized via AABB), circular floor,
//! dark environment, seeded orbiting lights and orbiting camera.

pub mod orbits;

use bevy::camera::primitives::Aabb;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::light::{DirectionalLight, PointLight, SpotLight};
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy_egui::EguiContexts;

use crate::app::AppSettings;
use crate::bench::{BenchState, LastError};
use crate::config::{DEFAULT_MODEL, Settings};
use orbits::{BenchClock, LightDeriveInput, LightParams, derive_lights};

pub struct ScenePlugin;

/// Marker: the benchmark camera (render settings are applied to it).
#[derive(Component)]
pub struct BenchCamera;

#[derive(Component)]
pub struct OrbitLight(pub usize);

#[derive(Component)]
pub struct BenchDirLight;

#[derive(Component)]
pub struct ModelRoot;

/// Root of the environment GLB (the spaceship room). Never normalized —
/// authored at world scale with its floor at y=0.
#[derive(Component)]
pub struct EnvRoot;

pub const ENVIRONMENT_MODEL: &str = "models/environment.glb";

/// Normalized model bounds; motion and framing derive from this.
#[derive(Resource)]
pub struct SceneMetrics {
    pub center: Vec3,
    pub radius: f32,
    pub ready: bool,
}

impl Default for SceneMetrics {
    fn default() -> Self {
        Self { center: Vec3::new(0.0, 0.9, 0.0), radius: 1.0, ready: false }
    }
}

#[derive(Resource, Default)]
pub struct LightRig {
    pub params: Vec<LightParams>,
}

/// User-controlled inspection camera (outside benchmark only, spec §5.4).
#[derive(Resource)]
pub struct FreeCam {
    pub active: bool,
    pub yaw: f64,
    pub pitch: f64,
    pub dist_mul: f64,
}

impl Default for FreeCam {
    fn default() -> Self {
        Self { active: false, yaw: 0.7, pitch: 0.45, dist_mul: 1.0 }
    }
}

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SceneMetrics>()
            .init_resource::<LightRig>()
            .init_resource::<BenchClock>()
            .init_resource::<FreeCam>()
            .insert_resource(ClearColor(Color::srgb(0.012, 0.014, 0.020)))
            .insert_resource(GlobalAmbientLight {
                color: Color::srgb(0.75, 0.82, 1.0),
                brightness: 12.0,
                ..default()
            })
            .add_systems(Startup, setup_static_scene)
            .add_systems(
                Update,
                (
                    orbits::tick_clock,
                    sync_model,
                    normalize_model,
                    rebuild_lights,
                    free_cam_input,
                    (move_lights, move_camera),
                    model_animation,
                    watch_asset_failures,
                )
                    .chain(),
            );
    }
}

// ---------------------------------------------------------------------------
// Static scene + model
// ---------------------------------------------------------------------------

fn setup_static_scene(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((
        BenchCamera,
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: 55f32.to_radians(),
            near: 0.1,
            far: 120.0,
            ..default()
        }),
        Transform::from_xyz(4.0, 2.2, 4.0).looking_at(Vec3::new(0.0, 0.9, 0.0), Vec3::Y),
    ));

    // Environment: circular spaceship room (white, black trim, ring lighting).
    // Provides the floor; authored at world scale, so no normalization.
    commands.spawn((
        EnvRoot,
        WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(ENVIRONMENT_MODEL))),
    ));
}

/// Spawns/replaces the model when the configured path changes (or at startup).
fn sync_model(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    settings: Res<AppSettings>,
    roots: Query<Entity, With<ModelRoot>>,
    mut metrics: ResMut<SceneMetrics>,
    mut current: Local<Option<Option<String>>>,
) {
    let wanted = settings.0.scene.model.clone();
    if current.as_ref() == Some(&wanted) {
        return;
    }
    *current = Some(wanted.clone());

    for e in &roots {
        commands.entity(e).despawn();
    }
    *metrics = SceneMetrics::default();

    let handle = match &wanted {
        None => asset_server.load(GltfAssetLabel::Scene(0).from_asset(DEFAULT_MODEL)),
        Some(path) => {
            asset_server.load(GltfAssetLabel::Scene(0).from_asset(std::path::PathBuf::from(path)))
        }
    };
    commands.spawn((ModelRoot, WorldAssetRoot(handle)));
    commands.insert_resource(CurrentModelPath(wanted.clone()));
    info!("loading model: {}", wanted.as_deref().unwrap_or(DEFAULT_MODEL));
}

/// Path of the model currently in the scene (None = bundled), for sub-asset loads.
#[derive(Resource, Default)]
pub struct CurrentModelPath(pub Option<String>);

/// Optional model animation (spec §5.1/§7.4): plays the GLB's first clip when
/// enabled and one exists; pauses when disabled. Never required by the model.
fn model_animation(
    settings: Res<AppSettings>,
    metrics: Res<SceneMetrics>,
    model_path: Option<Res<CurrentModelPath>>,
    roots: Query<Entity, With<ModelRoot>>,
    children: Query<&Children>,
    mut players: Query<&mut AnimationPlayer>,
    asset_server: Res<AssetServer>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut commands: Commands,
    mut wired: Local<bool>,
    mut warned: Local<bool>,
) {
    if !metrics.ready {
        *wired = false;
        return;
    }
    let enabled = settings.0.scene.animate_model;
    let Ok(root) = roots.single() else { return };

    for entity in children.iter_descendants(root) {
        let Ok(mut player) = players.get_mut(entity) else { continue };
        if !enabled {
            player.pause_all();
            continue;
        }
        if !*wired {
            *wired = true;
            let clip: Handle<AnimationClip> = match model_path.as_deref() {
                Some(CurrentModelPath(Some(path))) => asset_server
                    .load(GltfAssetLabel::Animation(0).from_asset(std::path::PathBuf::from(path))),
                _ => asset_server.load(GltfAssetLabel::Animation(0).from_asset(DEFAULT_MODEL)),
            };
            let (graph, node) = AnimationGraph::from_clip(clip);
            commands.entity(entity).insert(AnimationGraphHandle(graphs.add(graph)));
            player.play(node).repeat();
        }
        player.resume_all();
        return;
    }
    if enabled && !*warned {
        *warned = true;
        info!("model animation enabled but this GLB has no animations");
    }
}

/// Polls until mesh AABBs exist under the model root, then scales/centers the
/// model to a fixed extent and grounds it on the floor. Runs once per model.
fn normalize_model(
    mut metrics: ResMut<SceneMetrics>,
    roots: Query<Entity, With<ModelRoot>>,
    children: Query<&Children>,
    aabbs: Query<(&Aabb, &GlobalTransform), With<Mesh3d>>,
    mut transforms: Query<&mut Transform>,
    // Bounds must be identical on two consecutive frames before we trust them:
    // right after instancing, Aabbs can exist while GlobalTransforms are still
    // propagating, which yields garbage bounds (model ends up under the floor).
    mut pending: Local<Option<(Vec3, Vec3)>>,
) {
    if metrics.ready {
        return;
    }
    let Ok(root) = roots.single() else { return };

    let mut min = Vec3::MAX;
    let mut max = Vec3::MIN;
    let mut found = false;
    for e in children.iter_descendants(root) {
        if let Ok((aabb, gt)) = aabbs.get(e) {
            found = true;
            let center = Vec3::from(aabb.center);
            let half = Vec3::from(aabb.half_extents);
            for i in 0..8u8 {
                let corner = center
                    + half
                        * Vec3::new(
                            if i & 1 == 0 { -1.0 } else { 1.0 },
                            if i & 2 == 0 { -1.0 } else { 1.0 },
                            if i & 4 == 0 { -1.0 } else { 1.0 },
                        );
                let world = gt.transform_point(corner);
                min = min.min(world);
                max = max.max(world);
            }
        }
    }
    if !found {
        *pending = None;
        return; // scene not instanced / bounds not computed yet
    }
    match *pending {
        Some((pmin, pmax)) if pmin.abs_diff_eq(min, 1e-4) && pmax.abs_diff_eq(max, 1e-4) => {}
        _ => {
            debug!("model bounds candidate: min {min:?} max {max:?}");
            *pending = Some((min, max));
            return; // wait for bounds to stabilize
        }
    }
    *pending = None;

    let size = max - min;
    let max_extent = size.max_element().max(1e-3);
    let scale = 2.0 / max_extent;
    let center = (min + max) * 0.5;

    if let Ok(mut t) = transforms.get_mut(root) {
        t.scale = Vec3::splat(scale);
        t.translation = Vec3::new(-center.x * scale, -min.y * scale, -center.z * scale);
    }
    metrics.center = Vec3::new(0.0, size.y * 0.5 * scale, 0.0);
    metrics.radius = (size.length() * 0.5 * scale).max(0.6);
    metrics.ready = true;
    info!("model normalized: scale {scale:.3}, radius {:.2}", metrics.radius);
}

fn watch_asset_failures(
    mut failures: MessageReader<bevy::asset::AssetLoadFailedEvent<Gltf>>,
    mut last_error: ResMut<LastError>,
) {
    for failure in failures.read() {
        last_error.0 = Some(format!("failed to load model '{}': {}", failure.path, failure.error));
        error!("asset load failed: {} ({})", failure.path, failure.error);
    }
}

// ---------------------------------------------------------------------------
// Lights
// ---------------------------------------------------------------------------

#[derive(PartialEq, Clone)]
struct LightKey {
    seed: u32,
    count: u32,
    casters: u32,
    point_shadows: bool,
    spot_shadows: bool,
    shadows: bool,
    directional: bool,
    intensity: f32,
    speed: f32,
}

impl LightKey {
    fn from_settings(s: &Settings) -> Self {
        Self {
            seed: s.scene.seed,
            count: s.scene.light_count,
            casters: s.scene.shadow_caster_count,
            point_shadows: s.scene.point_shadows,
            spot_shadows: s.scene.spot_shadows,
            shadows: s.renderer.shadows,
            directional: s.scene.directional_light,
            intensity: s.scene.light_intensity,
            speed: s.scene.light_speed,
        }
    }
}

fn rebuild_lights(
    mut commands: Commands,
    settings: Res<AppSettings>,
    mut rig: ResMut<LightRig>,
    existing: Query<Entity, Or<(With<OrbitLight>, With<BenchDirLight>)>>,
    mut key: Local<Option<LightKey>>,
) {
    let new_key = LightKey::from_settings(&settings.0);
    if key.as_ref() == Some(&new_key) {
        return;
    }
    *key = Some(new_key);

    for e in &existing {
        commands.entity(e).despawn();
    }

    let s = &settings.0;
    rig.params = derive_lights(&LightDeriveInput {
        seed: s.scene.seed,
        count: s.scene.light_count,
        shadow_casters: s.scene.shadow_caster_count,
        point_shadows: s.scene.point_shadows,
        spot_shadows: s.scene.spot_shadows,
        shadows_enabled: s.renderer.shadows,
        intensity_mul: s.scene.light_intensity,
        speed_mul: s.scene.light_speed as f64,
    });

    for (i, p) in rig.params.iter().enumerate() {
        let color = Color::srgb(p.color[0], p.color[1], p.color[2]);
        let mut entity = commands.spawn((OrbitLight(i), Transform::default(), Visibility::default()));
        if p.spot {
            entity.insert(SpotLight {
                color,
                intensity: p.intensity,
                range: p.range,
                radius: p.light_radius,
                shadow_maps_enabled: p.shadows,
                inner_angle: 0.4,
                outer_angle: 0.8,
                ..default()
            });
        } else {
            entity.insert(PointLight {
                color,
                intensity: p.intensity,
                range: p.range,
                radius: p.light_radius,
                shadow_maps_enabled: p.shadows,
                ..default()
            });
        }
    }

    if s.scene.directional_light {
        commands.spawn((
            BenchDirLight,
            DirectionalLight {
                illuminance: 3_000.0,
                shadow_maps_enabled: s.renderer.shadows,
                ..default()
            },
            Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, 0.8, -0.9, 0.0)),
        ));
    }
}

fn move_lights(
    clock: Res<BenchClock>,
    rig: Res<LightRig>,
    metrics: Res<SceneMetrics>,
    mut lights: Query<(&OrbitLight, &mut Transform), Without<BenchCamera>>,
) {
    let look_target = metrics.center;
    for (OrbitLight(i), mut transform) in &mut lights {
        let Some(params) = rig.params.get(*i) else { continue };
        let pos = orbits::light_position(params, clock.t).as_vec3();
        transform.translation = pos;
        if params.spot {
            transform.look_at(look_target, Vec3::Y);
        }
    }
}

// ---------------------------------------------------------------------------
// Camera
// ---------------------------------------------------------------------------

fn move_camera(
    clock: Res<BenchClock>,
    metrics: Res<SceneMetrics>,
    settings: Res<AppSettings>,
    freecam: Res<FreeCam>,
    state: Res<State<BenchState>>,
    mut cameras: Query<&mut Transform, With<BenchCamera>>,
) {
    let Ok(mut transform) = cameras.single_mut() else { return };
    let center = metrics.center.as_dvec3();
    let radius = metrics.radius as f64;

    let (eye, target) = if freecam.active && !state.get().is_benchmarking() {
        let dist = (radius * 3.4).max(1.5) * freecam.dist_mul;
        let (ys, yc) = freecam.yaw.sin_cos();
        let (ps, pc) = freecam.pitch.sin_cos();
        (center + DVec3::new(dist * pc * yc, dist * ps, dist * pc * ys), center)
    } else {
        orbits::camera_pose(
            clock.t,
            center,
            radius,
            settings.0.scene.camera_distance as f64,
            settings.0.scene.camera_speed as f64,
        )
    };
    *transform = Transform::from_translation(eye.as_vec3()).looking_at(target.as_vec3(), Vec3::Y);
}

fn free_cam_input(
    mut contexts: EguiContexts,
    buttons: Res<ButtonInput<MouseButton>>,
    mut motion: MessageReader<MouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
    mut freecam: ResMut<FreeCam>,
    state: Res<State<BenchState>>,
) {
    if state.get().is_benchmarking() {
        freecam.active = false;
        motion.clear();
        wheel.clear();
        return;
    }
    // Don't fight egui for the pointer.
    let egui_wants = contexts
        .ctx_mut()
        .map(|ctx| ctx.egui_wants_pointer_input())
        .unwrap_or(false);
    if egui_wants {
        motion.clear();
        wheel.clear();
        return;
    }

    let scroll: f32 = wheel.read().map(|w| w.y).sum();
    if scroll != 0.0 {
        freecam.active = true;
        freecam.dist_mul = (freecam.dist_mul * (1.0 - scroll as f64 * 0.08)).clamp(0.3, 4.0);
    }
    if buttons.pressed(MouseButton::Left) || buttons.pressed(MouseButton::Right) {
        let delta: Vec2 = motion.read().map(|m| m.delta).sum();
        if delta != Vec2::ZERO {
            freecam.active = true;
            freecam.yaw += delta.x as f64 * 0.006;
            freecam.pitch = (freecam.pitch + delta.y as f64 * 0.004).clamp(-0.15, 1.4);
        }
    } else {
        motion.clear();
    }
}
