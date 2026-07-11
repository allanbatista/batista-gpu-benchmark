"""Builds the benchmark environment: a circular futuristic white spaceship
quarter with black trim and ring lighting. Exports to GLB (world scale:
radius 9 m, height 6 m — sized around the benchmark scene)."""

import bpy
import math
import sys

OUT = sys.argv[sys.argv.index("--") + 1]

bpy.ops.wm.read_factory_settings(use_empty=True)

R = 9.0      # room radius
H = 6.0      # room height


def make_mat(name, color, rough=0.5, metal=0.0, emission=None, strength=0.0, double_sided=False):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    bsdf = m.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*color, 1.0)
    bsdf.inputs["Roughness"].default_value = rough
    bsdf.inputs["Metallic"].default_value = metal
    if emission is not None:
        bsdf.inputs["Emission Color"].default_value = (*emission, 1.0)
        bsdf.inputs["Emission Strength"].default_value = strength
    m.use_backface_culling = not double_sided  # exporter maps to glTF doubleSided
    return m


WHITE_WALL = make_mat("wall_white", (0.86, 0.87, 0.90), rough=0.55, double_sided=True)
WHITE_FLOOR = make_mat("floor_white", (0.82, 0.83, 0.86), rough=0.42)
BLACK_TRIM = make_mat("trim_black", (0.015, 0.015, 0.018), rough=0.3, metal=0.2)
RING_LIGHT = make_mat("ring_light", (1.0, 1.0, 1.0), rough=0.4,
                      emission=(0.82, 0.90, 1.0), strength=40.0)
SOFT_LIGHT = make_mat("soft_light", (1.0, 1.0, 1.0), rough=0.4,
                      emission=(0.85, 0.92, 1.0), strength=18.0)
FLOOR_GLOW = make_mat("floor_glow", (1.0, 1.0, 1.0), rough=0.4,
                      emission=(0.55, 0.85, 1.0), strength=10.0)


def add(obj_name, mat):
    obj = bpy.context.active_object
    obj.name = obj_name
    obj.data.materials.append(mat)
    return obj


# Wall: open cylinder, seen from inside (double-sided material)
bpy.ops.mesh.primitive_cylinder_add(vertices=128, radius=R, depth=H, end_fill_type="NOTHING",
                                    location=(0, 0, H / 2))
add("wall", WHITE_WALL)

# Floor / ceiling discs
bpy.ops.mesh.primitive_circle_add(vertices=128, radius=R, fill_type="NGON", location=(0, 0, 0))
add("floor", WHITE_FLOOR)
bpy.ops.mesh.primitive_circle_add(vertices=128, radius=R, fill_type="NGON", location=(0, 0, H))
add("ceiling", WHITE_WALL)

# Black trims: baseboard + crown
bpy.ops.mesh.primitive_torus_add(major_radius=R - 0.12, minor_radius=0.10,
                                 major_segments=128, minor_segments=12, location=(0, 0, 0.10))
add("trim_base", BLACK_TRIM)
bpy.ops.mesh.primitive_torus_add(major_radius=R - 0.12, minor_radius=0.08,
                                 major_segments=128, minor_segments=12, location=(0, 0, H - 0.15))
add("trim_top", BLACK_TRIM)

# 12 black vertical ribs around the wall
for i in range(12):
    a = i * math.tau / 12
    x, y = (R - 0.18) * math.cos(a), (R - 0.18) * math.sin(a)
    bpy.ops.mesh.primitive_cube_add(location=(x, y, H / 2))
    rib = add(f"rib_{i}", BLACK_TRIM)
    rib.scale = (0.09, 0.20, H / 2)
    rib.rotation_euler = (0, 0, a)

# Door: black slab between two ribs
a = math.tau / 24  # centered between rib 0 and rib 1
bpy.ops.mesh.primitive_cube_add(location=((R - 0.25) * math.cos(a), (R - 0.25) * math.sin(a), 1.6))
door = add("door", BLACK_TRIM)
door.scale = (0.06, 1.1, 1.6)
door.rotation_euler = (0, 0, a)

# Main ceiling ring light
bpy.ops.mesh.primitive_torus_add(major_radius=5.4, minor_radius=0.10,
                                 major_segments=128, minor_segments=16, location=(0, 0, H - 0.28))
add("ring_light_main", RING_LIGHT)

# Ceiling inset: black ring housing + soft central disc
bpy.ops.mesh.primitive_torus_add(major_radius=2.1, minor_radius=0.09,
                                 major_segments=96, minor_segments=12, location=(0, 0, H - 0.08))
add("ceiling_housing", BLACK_TRIM)
bpy.ops.mesh.primitive_circle_add(vertices=96, radius=2.0, fill_type="NGON", location=(0, 0, H - 0.10))
ceiling_lamp = add("ceiling_lamp", SOFT_LIGHT)
ceiling_lamp.rotation_euler = (math.pi, 0, 0)  # face down

# Floor accent: thin glowing ring + black inlay ring
bpy.ops.mesh.primitive_torus_add(major_radius=6.6, minor_radius=0.045,
                                 major_segments=128, minor_segments=8, location=(0, 0, 0.02))
add("floor_glow_ring", FLOOR_GLOW)
bpy.ops.mesh.primitive_torus_add(major_radius=4.6, minor_radius=0.03,
                                 major_segments=128, minor_segments=8, location=(0, 0, 0.015))
add("floor_inlay", BLACK_TRIM)

bpy.ops.export_scene.gltf(filepath=OUT, export_format="GLB", export_apply=True)
print(f"exported: {OUT}")
