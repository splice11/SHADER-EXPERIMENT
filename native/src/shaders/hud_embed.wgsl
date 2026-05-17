// HUD embedding pass. Reads the finished composite scene + a separate
// egui-rendered HUD texture, and writes the final video frame with the HUD
// alpha attenuated by local scene brightness — bright cloud regions push the
// HUD toward invisible so the text reads as embedded behind the clouds
// rather than floating on top. egui-wgpu outputs premultiplied alpha, so
// we additively overlay the (already-premultiplied) hud RGB.

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_embed(@builtin(vertex_index) vid: u32) -> VsOut {
    let x = f32((vid << 1u) & 2u);
    let y = f32(vid & 2u);
    var out: VsOut;
    out.pos = vec4<f32>(x * 2.0 - 1.0, y * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(x, 1.0 - y);
    return out;
}

@group(0) @binding(0) var scene_tex: texture_2d<f32>;
@group(0) @binding(1) var scene_smp: sampler;
@group(0) @binding(2) var hud_tex:   texture_2d<f32>;
@group(0) @binding(3) var hud_smp:   sampler;

@fragment
fn fs_embed(in: VsOut) -> @location(0) vec4<f32> {
    let scene = textureSample(scene_tex, scene_smp, in.uv).rgb;
    let hud = textureSample(hud_tex, hud_smp, in.uv);

    // Average luma over a small 5-tap stencil so HUD visibility doesn't
    // shimmer as wisps drift through the text — we sense scene density
    // around the pixel rather than at it exactly.
    let off = 0.004;
    let l0 = dot(scene, vec3<f32>(0.2126, 0.7152, 0.0722));
    let l1 = dot(textureSample(scene_tex, scene_smp, in.uv + vec2<f32>( off,  0.0)).rgb,
                 vec3<f32>(0.2126, 0.7152, 0.0722));
    let l2 = dot(textureSample(scene_tex, scene_smp, in.uv + vec2<f32>(-off,  0.0)).rgb,
                 vec3<f32>(0.2126, 0.7152, 0.0722));
    let l3 = dot(textureSample(scene_tex, scene_smp, in.uv + vec2<f32>( 0.0,  off)).rgb,
                 vec3<f32>(0.2126, 0.7152, 0.0722));
    let l4 = dot(textureSample(scene_tex, scene_smp, in.uv + vec2<f32>( 0.0, -off)).rgb,
                 vec3<f32>(0.2126, 0.7152, 0.0722));
    let luma = (l0 + l1 + l2 + l3 + l4) * 0.2;

    // Bright scene → HUD almost gone; dark scene → full HUD. Floor at 0.10 so
    // text never *completely* disappears (would be confusing).
    let visible = max(0.10, 1.0 - smoothstep(0.10, 0.60, luma));
    let final_rgb = scene + hud.rgb * visible;
    return vec4<f32>(final_rgb, 1.0);
}
