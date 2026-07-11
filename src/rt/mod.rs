//! Experimental ray-traced lighting via bevy_solari (spec §4.2). Only compiled
//! with `--features rt-experimental`; only active when the adapter exposes the
//! required wgpu features (otherwise SolariLightingPlugin disables itself with
//! a warning). Results are always marked experimental/non-comparable — this
//! never silently replaces the PBR mode.

use bevy::camera::CameraMainTextureUsages;
use bevy::light::{PointLight, SpotLight};
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::TextureUsages;
use bevy::solari::prelude::{RaytracingMesh3d, SolariPlugins};
use bevy::solari::realtime::SolariLighting;

use crate::app::AppSettings;
use crate::config::RenderModeSetting;
use crate::scene::{BenchCamera, LightRig, OrbitLight};

/// Emissive radiance multiplier for the orbit-light spheres (Solari samples
/// emissive triangles as light sources; punctual lights are not supported).
const EMISSIVE_RADIANCE: f32 = 60_000.0;

#[derive(Component)]
struct RtLightViz;

pub struct RtPlugin;

impl Plugin for RtPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(SolariPlugins);
        app.add_systems(Startup, restore_forward_when_pbr);
        app.add_systems(Update, apply_rt_mode);
    }
}

/// SolariLightingPlugin::build forces the deferred renderer globally; in PBR
/// mode we restore the forward path so the official workload stays identical
/// to non-RT builds.
fn restore_forward_when_pbr(settings: Res<AppSettings>, mut commands: Commands) {
    if settings.0.renderer.render_mode == RenderModeSetting::Pbr {
        commands.insert_resource(bevy::pbr::DefaultOpaqueRendererMethod::forward());
    }
}

/// While in RT mode: register meshes with the BLAS builder, enable Solari on
/// the camera, disable punctual lights (Solari only samples directional lights
/// and emissive meshes) and attach emissive spheres to the orbit lights so the
/// moving-lights workload survives as real ray-traced light sources.
#[allow(clippy::too_many_arguments)]
fn apply_rt_mode(
    settings: Res<AppSettings>,
    rig: Res<LightRig>,
    cameras: Query<Entity, With<BenchCamera>>,
    meshes: Query<(Entity, &Mesh3d), Without<RaytracingMesh3d>>,
    bare_lights: Query<(Entity, &OrbitLight), Without<RtLightViz>>,
    mut point_lights: Query<&mut PointLight>,
    mut spot_lights: Query<&mut SpotLight>,
    mut mesh_assets: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
    mut sphere: Local<Option<Handle<Mesh>>>,
    mut active: Local<bool>,
) {
    let want_rt = settings.0.renderer.render_mode == RenderModeSetting::RtExperimental;
    let Ok(camera) = cameras.single() else { return };

    if want_rt {
        if !*active {
            *active = true;
            info!("experimental ray-traced lighting enabled (bevy_solari)");
            commands.entity(camera).insert((
                SolariLighting::default(),
                Msaa::Off,
                CameraMainTextureUsages::default().with(TextureUsages::STORAGE_BINDING),
            ));
        }
        // New meshes (model swaps, floor) keep getting registered while active.
        for (entity, mesh) in &meshes {
            // Solari requires UV_0 + tangents and u32 indices (same fixups as
            // bevy's official solari example) — otherwise meshes are invisible
            // to the raytracer.
            if let Some(mut mesh_asset) = mesh_assets.get_mut(&mesh.0) {
                make_solari_compatible(&mut mesh_asset);
            }
            commands.entity(entity).insert(RaytracingMesh3d(mesh.0.clone()));
        }
        // Punctual lights are invisible to Solari: silence them and let the
        // emissive spheres carry the lighting.
        for mut light in &mut point_lights {
            light.shadow_maps_enabled = false;
            light.intensity = 0.0;
        }
        for mut light in &mut spot_lights {
            light.shadow_maps_enabled = false;
            light.intensity = 0.0;
        }
        let sphere_handle = sphere
            .get_or_insert_with(|| mesh_assets.add(Sphere::new(0.12).mesh().ico(3).unwrap()))
            .clone();
        for (entity, OrbitLight(i)) in &bare_lights {
            let color = rig.params.get(*i).map(|p| p.color).unwrap_or([1.0, 1.0, 1.0]);
            let material = materials.add(StandardMaterial {
                base_color: Color::srgb(color[0], color[1], color[2]),
                emissive: LinearRgba::rgb(
                    color[0] * EMISSIVE_RADIANCE,
                    color[1] * EMISSIVE_RADIANCE,
                    color[2] * EMISSIVE_RADIANCE,
                ),
                ..default()
            });
            commands.entity(entity).insert((
                RtLightViz,
                Mesh3d(sphere_handle.clone()),
                MeshMaterial3d(material),
            ));
        }
    } else if *active {
        *active = false;
        commands.entity(camera).remove::<SolariLighting>();
    }
}

fn make_solari_compatible(mesh: &mut Mesh) {
    if !mesh.contains_attribute(Mesh::ATTRIBUTE_UV_0) {
        let vertex_count = mesh.count_vertices();
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; vertex_count]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, vec![[0.0, 0.0, 0.0, 0.0]; vertex_count]);
    }
    if !mesh.contains_attribute(Mesh::ATTRIBUTE_TANGENT) {
        let _ = mesh.generate_tangents();
    }
    if mesh.contains_attribute(Mesh::ATTRIBUTE_UV_1) {
        mesh.remove_attribute(Mesh::ATTRIBUTE_UV_1);
    }
    if let Some(indices) = mesh.indices_mut()
        && let Indices::U16(_) = indices
    {
        *indices = Indices::U32(indices.iter().map(|i| i as u32).collect());
    }
}
