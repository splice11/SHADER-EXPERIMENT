#version 300 es
// Cloud tunnel — based on Nimitz's "Protean Clouds" (Shadertoy 3l23Rh, MIT)
// with the camera path straightened and every interesting constant
// surfaced as a uniform so it can be driven from the UI / audio.
precision highp float;

in vec2 v_uv;
out vec4 frag;

uniform vec2  u_res;
uniform float u_time;

// ---- camera / motion ----------------------------------------------------
uniform float u_speed;          // forward speed along +Z
uniform float u_speedBass;      // bass-driven extra forward speed
uniform float u_fov;            // vertical fov in degrees
uniform float u_swayAmp;        // amplitude of original Nimitz pc_disp xy sway (0 = straight)
uniform float u_swayFreq;       // sway temporal frequency multiplier
uniform float u_extraSwayX;     // extra horizontal sine sway amplitude
uniform float u_rollAmp;        // world-rotation per-Z amplitude (0 = no rock)
uniform float u_rollFreq;       // world-rotation temporal freq

// ---- field shape --------------------------------------------------------
uniform float u_morph;          // base "prm1" morph factor 0..1
uniform float u_morphBass;      // bass mapped onto morph
uniform float u_morphCentroid;  // centroid mapped onto morph
uniform float u_density;        // overall density multiplier
uniform float u_densityBass;    // bass mapped onto density
uniform float u_noiseScale;     // input scale to the FBM field
uniform float u_dispAmp;        // micro-displacement amp inside the FBM loop
uniform float u_octaveScale;    // scale per octave inside FBM loop
uniform int   u_octaves;        // FBM octaves (3..7)

// ---- raymarch -----------------------------------------------------------
uniform int   u_steps;          // raymarch step count
uniform float u_stepSize;       // base step size
uniform float u_near;           // starting t
uniform float u_far;            // hard far clip

// ---- shading ------------------------------------------------------------
uniform vec3  u_colorA;         // shadow / deep colour
uniform vec3  u_colorB;         // lit / highlight colour
uniform vec3  u_colorAccent;    // accent (added on top)
uniform float u_accentAmt;
uniform float u_hueShift;       // extra hue rotation (radians)
uniform float u_hueCentroid;    // centroid amount onto hue
uniform vec3  u_fogColor;
uniform float u_fogDensity;
uniform float u_lightDir;       // light direction angle around Y (radians)
uniform float u_lightStrength;
uniform float u_hg;             // Henyey-Greenstein g (-1..1)

// ---- music --------------------------------------------------------------
uniform float u_bass;
uniform float u_mid;
uniform float u_treble;
uniform float u_centroid;
uniform float u_rms;
uniform float u_punch;          // transient envelope
uniform float u_beat;           // 0..1 beat phase

// ---- constants ----------------------------------------------------------
const float PI  = 3.14159265359;
const float TAU = 6.28318530718;

mat2 rot2(float a){ float c=cos(a), s=sin(a); return mat2(c,-s,s,c); }

// Hue rotation on linear RGB.
vec3 hueRotate(vec3 c, float a){
    const vec3 k = vec3(0.57735026919);
    float co = cos(a), si = sin(a);
    return c*co + cross(k, c)*si + k*dot(k, c)*(1.0 - co);
}

// ---- Nimitz Protean Clouds field ---------------------------------------
const mat3 PC_M3 = mat3(0.33338, 0.56034, -0.71817,
                       -0.87887, 0.32651, -0.15323,
                        0.15162, 0.69596,  0.61339) * 1.93;

float mag2(vec2 p){ return dot(p,p); }
float linstep(float a, float b, float x){ return clamp((x-a)/(b-a), 0.0, 1.0); }
mat2 prot(float a){ float c=cos(a), s=sin(a); return mat2(c,s,-s,c); }
vec2 disp(float t){ return vec2(sin(t*0.22*u_swayFreq), cos(t*0.175*u_swayFreq)) * u_swayAmp; }

// Returns vec2(density, radial). p is in world space.
vec2 field(vec3 p, float prm1){
    vec3 p2 = p;
    p2.xy -= disp(p.z);
    // World-rocking rotation is opt-in via u_rollAmp (0 disables jerky look).
    p.xy *= prot(sin(p.z + u_time*u_rollFreq) * (u_rollAmp + prm1*u_rollAmp*0.5));
    float cl = mag2(p2.xy);
    float d = 0.0;
    p *= u_noiseScale;
    float z = 1.0;
    float trk = 1.0;
    float dispAmp = u_dispAmp + prm1*0.2;
    for (int i = 0; i < 8; i++){
        if (i >= u_octaves) break;
        p += sin(p.zxy*0.75*trk + u_time*trk*0.8) * dispAmp;
        d -= abs(dot(cos(p), sin(p.yzx)) * z);
        z *= u_octaveScale;
        trk *= 1.4;
        p = p * PC_M3;
    }
    d = abs(d + prm1*3.0) + prm1*0.3 - 2.5;
    return vec2(d + cl*0.2 + 0.25, cl);
}

// Henyey-Greenstein phase function.
float hgPhase(float costh, float g){
    float g2 = g*g;
    return (1.0 - g2) / (4.0 * PI * pow(1.0 + g2 - 2.0*g*costh, 1.5));
}

// Volumetric raymarch with crude self-shadowing (one extra sample per step).
vec4 march(vec3 ro, vec3 rd, float prm1, vec3 lightDir){
    vec4 rez = vec4(0.0);
    float t = u_near;
    float fogT = 0.0;
    float phase = mix(1.0/(4.0*PI), hgPhase(dot(rd, lightDir), u_hg), 1.0);
    for (int i = 0; i < 256; i++){
        if (i >= u_steps) break;
        if (rez.a > 0.99) break;
        vec3 pos = ro + t*rd;
        vec2 mp = field(pos, prm1);
        float den = clamp(mp.x - 0.3, 0.0, 1.0) * u_density;
        float dn  = clamp(mp.x + 2.0, 0.0, 3.0);
        vec4 col = vec4(0.0);
        if (mp.x > 0.6){
            // cheap shading: gradient-of-density toward light
            float dif1 = clamp((den - field(pos + lightDir*0.8, prm1).x) / 9.0, 0.001, 1.0);
            float dif2 = clamp((den - field(pos + lightDir*0.35, prm1).x) / 2.5, 0.001, 1.0);
            float shade = (dif1 + dif2) * u_lightStrength * phase * 4.0;

            // base palette ramp by radial / depth
            vec3 base = mix(u_colorA, u_colorB, smoothstep(0.0, 1.0, den));
            base += u_colorAccent * u_accentAmt
                  * (0.5 + 0.5*sin(vec3(5.0, 0.4, 0.2) + mp.y*0.1 + pos.z*0.4)).x;

            col.rgb = base * (0.20 + shade);
            col.a   = den * 0.10;
            col.rgb *= den * den * den * 3.0;
            col.rgb *= linstep(4.0, -2.5, mp.x) * 2.3;
        }
        // fog (exp depth)
        float fogC = exp(t * u_fogDensity - 2.2);
        col.rgba += vec4(u_fogColor, 0.10) * clamp(fogC - fogT, 0.0, 1.0);
        fogT = fogC;
        rez = rez + col * (1.0 - rez.a);
        t += clamp(u_stepSize - dn*dn*0.05, 0.09, 0.3);
        if (t > u_far) break;
    }
    return clamp(rez, 0.0, 1.0);
}

void main(){
    vec2 uv = v_uv;
    vec2 p  = (uv - 0.5) * vec2(u_res.x/u_res.y, 1.0);

    // --- morph factor (replaces Nimitz's prm1) ---
    float prm1 = clamp(u_morph + u_morphBass*u_bass + u_morphCentroid*u_centroid, 0.0, 1.0);

    // --- monotonic forward time ---
    float fwd = u_time * (u_speed + u_speedBass*u_bass);

    // --- camera: straight along +Z by default, optional gentle sway ---
    vec3 ro = vec3(0.0, 0.0, fwd);
    ro.xy += disp(ro.z);                                   // 0 if u_swayAmp == 0
    ro.x  += sin(u_time*0.7) * u_extraSwayX;               // 0 if u_extraSwayX == 0

    // Build a camera that always looks down +Z toward a stable target.
    float tgtDst = 3.5;
    vec3 target = vec3(0.0, 0.0, fwd + tgtDst);
    target.xy += disp(target.z);
    vec3 fwdDir = normalize(target - ro);
    vec3 rightDir = normalize(cross(fwdDir, vec3(0.0, 1.0, 0.0)));
    vec3 upDir    = normalize(cross(rightDir, fwdDir));
    float fovScale = 1.0 / tan(radians(u_fov) * 0.5);
    vec3 rd = normalize(p.x*rightDir + p.y*upDir + fwdDir*fovScale);

    // light direction in world XZ plane
    float la = u_lightDir;
    vec3 lightDir = normalize(vec3(cos(la), 0.35, sin(la)));

    vec4 scn = march(ro, rd, prm1, lightDir);
    // bass-driven brightness/density mult on the result.
    scn.rgb *= 1.0 + u_densityBass * u_bass;

    // colour-space hue twist driven by centroid/punch
    float hue = u_hueShift + u_hueCentroid*(u_centroid - 0.5) + 0.15*u_punch*sin(u_beat*TAU);
    scn.rgb = hueRotate(scn.rgb, hue);

    // beat-phase gentle brightness lift
    scn.rgb *= 1.0 + 0.06*(1.0 - u_beat);

    frag = vec4(scn.rgb, 1.0);
}
