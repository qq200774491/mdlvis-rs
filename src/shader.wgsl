// Vertex shader
struct CameraUniform {
    view_proj: mat4x4<f32>,
};

struct MaterialUniform {
    team_color: vec4<f32>, // team_color.rgb + replaceable_id (0=none, 1=team_color, 2=team_glow)
    material_type_and_wireframe: vec4<f32>, // filter_mode + wireframe_mode + layer_alpha + shading_flags
    extra_padding: vec4<f32>, // Padding for alignment
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(1) @binding(1)
var s_diffuse: sampler;

@group(2) @binding(0)
var<uniform> material: MaterialUniform;

struct SceneDrawUniform {
    legacy_team_color: vec4<f32>,
    legacy_material: vec4<f32>,
    legacy_padding: vec4<f32>,
    geoset_color_alpha: vec4<f32>,
    layer_options: vec4<f32>, // layer alpha, filter, render bits, texture slot
    texture_transform: mat4x4<f32>,
};

@group(3) @binding(0)
var<uniform> scene_draw: SceneDrawUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

@vertex
fn vs_main(
    model: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;
    // Apply Y-axis inversion like Delphi glScalef(1.0, -1.0, 1.0)
    var pos = model.position;
    pos.y = -pos.y;
    out.world_pos = pos;
    out.clip_position = camera.view_proj * vec4<f32>(pos, 1.0);
    out.normal = model.normal;
    out.uv = model.uv;
    return out;
}

// Fragment shader with texture sampling
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Extract values from uniform structure
    let team_color_rgb = material.team_color.xyz;
    let replaceable_id = material.team_color.w; // 0=none, 1=team_color, 2=team_glow
    let filter_mode = material.material_type_and_wireframe.x;
    let wireframe_mode = material.material_type_and_wireframe.y;
    let layer_alpha = material.material_type_and_wireframe.z;
    let shading_flags = u32(material.material_type_and_wireframe.w);
    
    // Check if unshaded flag is set (0x1)
    let is_unshaded = (shading_flags & 0x1u) != 0u;
    
    // In wireframe mode, use a solid color instead of texture
    if (wireframe_mode > 0.5) {
        var wireframe_color = vec3<f32>(0.0, 1.0, 0.0); // Default green
        if (filter_mode >= 4.0) { // Additive or AddAlpha (now 4.0+)
            wireframe_color = vec3<f32>(1.0, 0.5, 0.0); // Orange for glow effects
        }
        return vec4<f32>(wireframe_color, 1.0);
    }
    
    // Sample texture (for RID=1/2 textures are already generated with team color)
    var tex_color = textureSample(t_diffuse, s_diffuse, in.uv);
    
    // Filter mode handling:
    // 0 = None - no transparency
    // 1 = Transparent - alpha testing (discard transparent pixels)
    // 2 = Blend - alpha blending
    // 3+ = Additive/etc
    
    // Alpha test for Transparent mode - War3 uses cutout, not blending!
    if (filter_mode > 0.5 && filter_mode < 1.5) { // Transparent mode
        if (tex_color.a < 0.01) { // Discard nearly transparent pixels
            discard;
        }
        // Don't modify alpha - keep original for proper rendering
    }

    
    // Apply layer_alpha to texture
    var layer_tex_color = tex_color;
    layer_tex_color.a = tex_color.a * layer_alpha;
    
    // Apply lighting only to non-glow materials AND if not unshaded
    var final_color = layer_tex_color;
    if (filter_mode < 4.0 && !is_unshaded) { // Not additive/glow (now < 4.0) AND not unshaded
        let light_dir = normalize(vec3<f32>(1.0, 1.0, 1.0));
        var normal = normalize(in.normal);
        normal.y = -normal.y; // Flip normal Y too
        let diffuse = max(dot(normal, light_dir), 0.0);
        let ambient = 0.3;
        let brightness = ambient + (1.0 - ambient) * diffuse;
        final_color = vec4<f32>(layer_tex_color.rgb * brightness, layer_tex_color.a);
    }
    
    return final_color;
}

@vertex
fn vs_scene(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    var pos = model.position;
    pos.y = -pos.y;
    out.world_pos = pos;
    out.clip_position = camera.view_proj * vec4<f32>(pos, 1.0);
    out.normal = model.normal;
    out.uv = model.uv;
    return out;
}

@fragment
fn fs_scene(in: VertexOutput) -> @location(0) vec4<f32> {
    let layer_alpha = scene_draw.layer_options.x;
    let filter_mode = scene_draw.layer_options.y;
    let render_bits = u32(scene_draw.layer_options.z);
    let unshaded = (render_bits & 1u) != 0u;
    let sphere_env_map = (render_bits & 2u) != 0u;
    let straight_required = scene_draw.legacy_padding.x > 0.5;

    var base_uv = in.uv;
    if (sphere_env_map) {
        let sphere_normal = normalize(vec3<f32>(in.normal.x, -in.normal.y, in.normal.z));
        let eye_delta = scene_draw.legacy_team_color.xyz - in.world_pos;
        let eye_distance_squared = dot(eye_delta, eye_delta);
        var view_dir = vec3<f32>(0.0, 0.0, 1.0);
        if (eye_distance_squared > 0.0000001) {
            view_dir = eye_delta * inverseSqrt(eye_distance_squared);
        }
        let reflected = reflect(-view_dir, sphere_normal);
        let denominator = max(2.0 * sqrt(reflected.x * reflected.x + reflected.y * reflected.y + (reflected.z + 1.0) * (reflected.z + 1.0)), 0.00001);
        base_uv = reflected.xy / denominator + vec2<f32>(0.5, 0.5);
    }
    let sample_uv = (scene_draw.texture_transform * vec4<f32>(base_uv, 0.0, 1.0)).xy;
    let tex_color = textureSample(t_diffuse, s_diffuse, sample_uv);
    let combined_alpha = tex_color.a * scene_draw.geoset_color_alpha.a * layer_alpha;
    if (scene_draw.geoset_color_alpha.a < 0.01 || layer_alpha < 0.01) {
        discard;
    }
    if (filter_mode > 0.5 && filter_mode < 1.5 && combined_alpha < 0.75) {
        discard;
    }

    let geoset_color = scene_draw.geoset_color_alpha.rgb;
    var rgb = tex_color.rgb * geoset_color;
    if (straight_required && tex_color.a > 0.00001) {
        rgb /= tex_color.a;
    }
    if (!unshaded) {
        let light_dir = normalize(vec3<f32>(1.0, 1.0, 1.0));
        let normal = normalize(vec3<f32>(in.normal.x, -in.normal.y, in.normal.z));
        let diffuse = max(dot(normal, light_dir), 0.0);
        rgb *= 0.3 + 0.7 * diffuse;
    }
    // ONE/ONE does not use output alpha. Original Real3D multiplies additive RGB by AAlpha.
    if (filter_mode > 2.5) {
        rgb *= layer_alpha;
    }
    return vec4<f32>(rgb, combined_alpha);
}

// Line rendering shaders
struct LineVertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
};

struct LineVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_line(
    model: LineVertexInput,
) -> LineVertexOutput {
    var out: LineVertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(model.position, 1.0);
    out.color = model.color;
    return out;
}

@fragment
fn fs_line(in: LineVertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
