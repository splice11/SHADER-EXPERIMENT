// Tiny WebGL2 renderer: scene → HDR FBO with mipmaps → composite to screen.

import { PARAM_DEFS } from './params.js';

const UNIFORM_CACHE = new WeakMap();

function compile(gl, type, src, name) {
  const sh = gl.createShader(type);
  gl.shaderSource(sh, src);
  gl.compileShader(sh);
  if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) {
    const log = gl.getShaderInfoLog(sh) || '';
    throw new Error(`shader compile (${name}):\n${log}\n--- source ---\n${src}`);
  }
  return sh;
}

function link(gl, vs, fs, name) {
  const p = gl.createProgram();
  gl.attachShader(p, vs);
  gl.attachShader(p, fs);
  gl.linkProgram(p);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) {
    throw new Error(`program link (${name}): ${gl.getProgramInfoLog(p)}`);
  }
  return p;
}

function uloc(gl, prog, name) {
  let m = UNIFORM_CACHE.get(prog);
  if (!m) { m = new Map(); UNIFORM_CACHE.set(prog, m); }
  if (m.has(name)) return m.get(name);
  const l = gl.getUniformLocation(prog, name);
  m.set(name, l);
  return l;
}

export class Renderer {
  constructor(canvas) {
    this.canvas = canvas;
    const gl = canvas.getContext('webgl2', { alpha: false, antialias: false, premultipliedAlpha: false });
    if (!gl) throw new Error('WebGL2 not available');
    this.gl = gl;
    if (!gl.getExtension('EXT_color_buffer_float'))
      throw new Error('EXT_color_buffer_float required');
    // For mipmapping a float texture (used by bloom).
    gl.getExtension('OES_texture_float_linear');

    this.vao = gl.createVertexArray();   // empty VAO; vertices come from gl_VertexID
    this.scenePass = null;
    this.compositePass = null;

    this.sceneFbo = null;
    this.sceneTex = null;
    this.w = 0; this.h = 0;
    this.dpr = Math.min(window.devicePixelRatio || 1, 2);
  }

  async init(sceneFragSrc, compositeFragSrc, vertSrc) {
    const gl = this.gl;
    const vs = compile(gl, gl.VERTEX_SHADER, vertSrc, 'fullscreen.vert');
    this.scenePass     = link(gl, vs, compile(gl, gl.FRAGMENT_SHADER, sceneFragSrc,    'scene.frag'),     'scene');
    this.compositePass = link(gl, vs, compile(gl, gl.FRAGMENT_SHADER, compositeFragSrc, 'composite.frag'), 'composite');
  }

  resize(w, h) {
    const gl = this.gl;
    const pw = Math.max(1, Math.floor(w * this.dpr));
    const ph = Math.max(1, Math.floor(h * this.dpr));
    if (pw === this.w && ph === this.h) return;
    this.w = pw; this.h = ph;
    this.canvas.width = pw; this.canvas.height = ph;
    if (this.sceneTex) gl.deleteTexture(this.sceneTex);
    if (this.sceneFbo) gl.deleteFramebuffer(this.sceneFbo);
    this.sceneTex = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, this.sceneTex);
    // 8 mip levels covers the LOD range our bloom samples; clamp to what fits.
    const maxLevels = Math.floor(Math.log2(Math.max(pw, ph))) + 1;
    const useLevels = Math.min(maxLevels, 8);
    gl.texStorage2D(gl.TEXTURE_2D, useLevels, gl.RGBA16F, pw, ph);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    this.sceneFbo = gl.createFramebuffer();
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.sceneFbo);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, this.sceneTex, 0);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  }

  // Sets uniforms for every param def on the bound program.
  _setStateUniforms(prog, state, group) {
    const gl = this.gl;
    for (const [k, def] of Object.entries(PARAM_DEFS)) {
      if (def.group !== group) continue;
      const name = def.uniform || ('u_' + k);
      const loc = uloc(gl, prog, name);
      if (!loc) continue;
      const v = state[k];
      switch (def.type) {
        case 'float':
        case 'angle': gl.uniform1f(loc, v); break;
        case 'int':   gl.uniform1i(loc, v|0); break;
        case 'color': gl.uniform3f(loc, v[0], v[1], v[2]); break;
      }
    }
  }

  render(state, audio, time) {
    const gl = this.gl;

    // ---- pass 1: scene to HDR FBO --------------------------------------
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.sceneFbo);
    gl.viewport(0, 0, this.w, this.h);
    gl.useProgram(this.scenePass);
    gl.uniform2f(uloc(gl, this.scenePass, 'u_res'), this.w, this.h);
    gl.uniform1f(uloc(gl, this.scenePass, 'u_time'), time);
    // audio features
    gl.uniform1f(uloc(gl, this.scenePass, 'u_bass'),     audio.bass);
    gl.uniform1f(uloc(gl, this.scenePass, 'u_mid'),      audio.mid);
    gl.uniform1f(uloc(gl, this.scenePass, 'u_treble'),   audio.treble);
    gl.uniform1f(uloc(gl, this.scenePass, 'u_centroid'), audio.centroid);
    gl.uniform1f(uloc(gl, this.scenePass, 'u_rms'),      audio.rms);
    gl.uniform1f(uloc(gl, this.scenePass, 'u_punch'),    audio.punch);
    gl.uniform1f(uloc(gl, this.scenePass, 'u_beat'),     audio.beat);
    this._setStateUniforms(this.scenePass, state, 'motion');
    this._setStateUniforms(this.scenePass, state, 'field');
    this._setStateUniforms(this.scenePass, state, 'raymarch');
    this._setStateUniforms(this.scenePass, state, 'shading');
    gl.bindVertexArray(this.vao);
    gl.drawArrays(gl.TRIANGLES, 0, 3);

    // mipmaps for bloom
    gl.bindTexture(gl.TEXTURE_2D, this.sceneTex);
    gl.generateMipmap(gl.TEXTURE_2D);

    // ---- pass 2: composite to screen -----------------------------------
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.viewport(0, 0, this.w, this.h);
    gl.useProgram(this.compositePass);
    gl.uniform2f(uloc(gl, this.compositePass, 'u_res'), this.w, this.h);
    gl.uniform1f(uloc(gl, this.compositePass, 'u_time'), time);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, this.sceneTex);
    gl.uniform1i(uloc(gl, this.compositePass, 'u_scene'), 0);
    this._setStateUniforms(this.compositePass, state, 'post');
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  }
}
