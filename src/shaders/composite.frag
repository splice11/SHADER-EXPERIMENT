#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 frag;

uniform sampler2D u_scene;      // HDR scene, with mipmaps generated
uniform vec2  u_res;
uniform float u_time;

uniform float u_exposure;
uniform float u_bloomAmt;
uniform float u_bloomThreshold;
uniform float u_ca;             // chromatic aberration amount
uniform float u_vignette;
uniform float u_grain;
uniform float u_gamma;
uniform float u_contrast;
uniform float u_saturation;

vec3 aces(vec3 x){
    const float a=2.51, b=0.03, c=2.43, d=0.59, e=0.14;
    return clamp((x*(a*x+b))/(x*(c*x+d)+e), 0.0, 1.0);
}

float hash12(vec2 p){
    vec3 p3 = fract(vec3(p.xyx) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

// Cheap bloom: sample several mip levels and accumulate the bright part.
vec3 bloomSample(vec2 uv){
    vec3 acc = vec3(0.0);
    float wsum = 0.0;
    for (int i = 1; i <= 6; i++){
        float lod = float(i) + 0.5;
        vec3 s = textureLod(u_scene, uv, lod).rgb;
        float b = max(max(s.r, s.g), s.b);
        float w = smoothstep(u_bloomThreshold, u_bloomThreshold + 0.4, b);
        acc += s * w;
        wsum += 1.0;
    }
    return acc / max(wsum, 1.0);
}

void main(){
    vec2 uv = v_uv;
    vec2 dir = uv - 0.5;
    float r2 = dot(dir, dir);

    // Chromatic aberration on the scene sample.
    vec2 ofs = dir * u_ca;
    vec3 scene;
    scene.r = textureLod(u_scene, uv + ofs, 0.0).r;
    scene.g = textureLod(u_scene, uv,        0.0).g;
    scene.b = textureLod(u_scene, uv - ofs,  0.0).b;

    vec3 col = scene + bloomSample(uv) * u_bloomAmt;
    col *= u_exposure;
    col = aces(col);

    // saturation
    float l = dot(col, vec3(0.2126, 0.7152, 0.0722));
    col = mix(vec3(l), col, u_saturation);
    // contrast about 0.5
    col = (col - 0.5) * u_contrast + 0.5;

    col *= 1.0 - r2 * u_vignette;

    // triangular-PDF grain
    vec2 gp = uv * u_res;
    float r1 = hash12(gp + u_time*60.0);
    float r2g = hash12(gp + u_time*60.0 + 113.7);
    float g = (r1 + r2g) - 1.0;
    col += vec3(g) * u_grain;

    col = pow(max(col, 0.0), vec3(1.0/u_gamma));
    frag = vec4(col, 1.0);
}
