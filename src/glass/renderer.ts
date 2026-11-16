import { BLUR_FRAGMENT, GLASS_FRAGMENT, MAX_SHAPES, QUAD_VERTEX } from "./shader";

/** Tunable look of the glass. Lengths are CSS pixels. */
export interface GlassParams {
  refraction: number;
  edgeWidth: number;
  /** 0 clear .. 1 frosted. */
  blur: number;
  chroma: number;
  highlight: number;
  rim: number;
  fresnel: number;
  saturation: number;
  brightness: number;
  /** Tint colour, 0..1 per channel. */
  tint: [number, number, number];
  tintStrength: number;
  /** 0 = glass, 1 = solid tint (the opaque themes). */
  opacity: number;
  shadow: number;
}

/** A glass surface in CSS pixels relative to the window. */
export interface GlassShape {
  x: number;
  y: number;
  w: number;
  h: number;
  /** Corner radius. */
  r: number;
  /** Extra frost for this surface on top of the theme's, 0..1. For surfaces that carry text. */
  frost?: number;
}

const SHADOW_MARGIN = 48; // CSS pixels around the shapes that can receive shadow

function compile(gl: WebGL2RenderingContext, type: number, source: string): WebGLShader {
  const shader = gl.createShader(type)!;
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    throw new Error(`shader compile failed: ${gl.getShaderInfoLog(shader)}`);
  }
  return shader;
}

function link(gl: WebGL2RenderingContext, fragment: string): WebGLProgram {
  const program = gl.createProgram()!;
  gl.attachShader(program, compile(gl, gl.VERTEX_SHADER, QUAD_VERTEX));
  gl.attachShader(program, compile(gl, gl.FRAGMENT_SHADER, fragment));
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    throw new Error(`program link failed: ${gl.getProgramInfoLog(program)}`);
  }
  return program;
}

/**
 * Draws the glass. It never runs a loop of its own: `requestRender` schedules
 * exactly one frame, so an idle dock costs nothing.
 */
export class GlassRenderer {
  private gl: WebGL2RenderingContext;
  private program: WebGLProgram;
  private blurProgram: WebGLProgram;
  private uniforms = new Map<string, WebGLUniformLocation | null>();
  private sharp: WebGLTexture | null = null;
  private blurred: WebGLTexture | null = null;
  private monitor: [number, number] = [1, 1];
  private origin: [number, number] = [0, 0];
  private shapes: GlassShape[] = [];
  private blend = 0;
  private params: GlassParams;
  private fallback: [number, number, number] = [0.2, 0.2, 0.22];
  private frame = 0;
  private disposed = false;

  constructor(
    private canvas: HTMLCanvasElement,
    params: GlassParams,
  ) {
    const gl = canvas.getContext("webgl2", {
      alpha: true,
      premultipliedAlpha: true,
      antialias: false,
      depth: false,
      stencil: false,
      powerPreference: "low-power",
    });
    if (!gl) throw new Error("WebGL2 is not available");
    this.gl = gl;
    this.params = params;
    this.program = link(gl, GLASS_FRAGMENT);
    this.blurProgram = link(gl, BLUR_FRAGMENT);
    gl.bindVertexArray(gl.createVertexArray());
    gl.disable(gl.DEPTH_TEST);
    gl.disable(gl.BLEND);
  }

  private uniform(name: string): WebGLUniformLocation | null {
    if (!this.uniforms.has(name)) {
      this.uniforms.set(name, this.gl.getUniformLocation(this.program, name));
    }
    return this.uniforms.get(name)!;
  }

  private texture(width: number, height: number, pixels: Uint8Array | null, mips: boolean) {
    const gl = this.gl;
    const texture = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, width, height, 0, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(
      gl.TEXTURE_2D,
      gl.TEXTURE_MIN_FILTER,
      mips ? gl.LINEAR_MIPMAP_LINEAR : gl.LINEAR,
    );
    return texture;
  }

  /**
   * Set the wallpaper pixels (RGBA, rows top to bottom), or null when the
   * wallpaper is unavailable. Builds the frosted copy once, here, so drawing
   * a frame never blurs anything.
   */
  setBackdrop(pixels: Uint8Array | null, width: number, height: number) {
    const gl = this.gl;
    if (this.sharp) gl.deleteTexture(this.sharp);
    if (this.blurred) gl.deleteTexture(this.blurred);
    this.sharp = this.blurred = null;
    if (!pixels || width <= 0 || height <= 0) {
      this.requestRender();
      return;
    }
    this.monitor = [width, height];
    this.sharp = this.texture(width, height, pixels, false);

    // Quarter-resolution Gaussian: horizontal into a scratch texture, then
    // vertical into the final one, then mipmaps for stronger frost levels.
    const bw = Math.max(1, Math.round(width / 4));
    const bh = Math.max(1, Math.round(height / 4));
    const scratch = this.texture(bw, bh, null, false);
    this.blurred = this.texture(bw, bh, null, true);
    const framebuffer = gl.createFramebuffer();
    gl.bindFramebuffer(gl.FRAMEBUFFER, framebuffer);
    gl.useProgram(this.blurProgram);
    gl.viewport(0, 0, bw, bh);
    gl.disable(gl.SCISSOR_TEST);
    gl.activeTexture(gl.TEXTURE0);
    gl.uniform1i(gl.getUniformLocation(this.blurProgram, "uSource"), 0);
    gl.uniform2f(gl.getUniformLocation(this.blurProgram, "uSize"), bw, bh);
    const texel = gl.getUniformLocation(this.blurProgram, "uTexel");
    const pass = (source: WebGLTexture, target: WebGLTexture, dx: number, dy: number) => {
      gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, target, 0);
      gl.bindTexture(gl.TEXTURE_2D, source);
      gl.uniform2f(texel, dx, dy);
      gl.drawArrays(gl.TRIANGLES, 0, 3);
    };
    pass(this.sharp, scratch, 1.6 / bw, 0);
    pass(scratch, this.blurred, 0, 1.6 / bh);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.deleteFramebuffer(framebuffer);
    gl.deleteTexture(scratch);
    gl.bindTexture(gl.TEXTURE_2D, this.blurred);
    gl.generateMipmap(gl.TEXTURE_2D);
    this.requestRender();
  }

  /** Window origin on the monitor, in device pixels. */
  setOrigin(x: number, y: number) {
    this.origin = [x, y];
    this.requestRender();
  }

  setShapes(shapes: GlassShape[], blend: number) {
    this.shapes = shapes.slice(0, MAX_SHAPES);
    this.blend = blend;
    this.requestRender();
  }

  setParams(params: GlassParams) {
    this.params = params;
    this.requestRender();
  }

  /** Base colour used when there is no wallpaper to refract. */
  setFallback(color: [number, number, number]) {
    this.fallback = color;
    this.requestRender();
  }

  requestRender() {
    if (this.frame || this.disposed) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = 0;
      this.render();
    });
  }

  private render() {
    const { gl, canvas } = this;
    const scale = window.devicePixelRatio || 1;
    // The fractional CSS size, not clientWidth: that is rounded to a whole
    // CSS pixel and would put the canvas a device pixel off at 150% scaling.
    const box = canvas.getBoundingClientRect();
    const width = Math.round(box.width * scale);
    const height = Math.round(box.height * scale);
    if (width <= 0 || height <= 0) return;
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
    }

    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.viewport(0, 0, width, height);
    gl.disable(gl.SCISSOR_TEST);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    if (this.shapes.length === 0) return;

    // Only shade the part of the window the glass and its shadow can touch.
    let left = Infinity;
    let top = Infinity;
    let right = -Infinity;
    let bottom = -Infinity;
    for (const s of this.shapes) {
      left = Math.min(left, s.x);
      top = Math.min(top, s.y);
      right = Math.max(right, s.x + s.w);
      bottom = Math.max(bottom, s.y + s.h);
    }
    const p = this.params;
    const margin = SHADOW_MARGIN + this.blend;
    const x0 = Math.max(0, Math.floor((left - margin) * scale));
    const y0 = Math.max(0, Math.floor((top - margin) * scale));
    const x1 = Math.min(width, Math.ceil((right + margin) * scale));
    const y1 = Math.min(height, Math.ceil((bottom + margin) * scale));
    if (x1 <= x0 || y1 <= y0) return;
    gl.enable(gl.SCISSOR_TEST);
    gl.scissor(x0, height - y1, x1 - x0, y1 - y0);

    gl.useProgram(this.program);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, this.sharp);
    gl.uniform1i(this.uniform("uBackdrop"), 0);
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, this.blurred);
    gl.uniform1i(this.uniform("uBlurred"), 1);
    gl.uniform1f(this.uniform("uHasBackdrop"), this.sharp ? 1 : 0);
    gl.uniform2f(this.uniform("uMonitor"), this.monitor[0], this.monitor[1]);
    gl.uniform2f(this.uniform("uOrigin"), this.origin[0], this.origin[1]);
    gl.uniform2f(this.uniform("uCanvas"), width, height);
    gl.uniform1f(this.uniform("uScale"), scale);

    const rects = new Float32Array(MAX_SHAPES * 4);
    const radii = new Float32Array(MAX_SHAPES);
    const frost = new Float32Array(MAX_SHAPES);
    this.shapes.forEach((s, i) => {
      rects.set(
        [(s.x + s.w / 2) * scale, (s.y + s.h / 2) * scale, (s.w / 2) * scale, (s.h / 2) * scale],
        i * 4,
      );
      radii[i] = s.r * scale;
      // Opaque themes have nothing to frost.
      frost[i] = p.opacity >= 1 ? 0 : (s.frost ?? 0);
    });
    gl.uniform1i(this.uniform("uCount"), this.shapes.length);
    gl.uniform4fv(this.uniform("uRect[0]"), rects);
    gl.uniform1fv(this.uniform("uRadius[0]"), radii);
    gl.uniform1fv(this.uniform("uExtraFrost[0]"), frost);
    gl.uniform1f(this.uniform("uBlend"), this.blend * scale);

    gl.uniform1f(this.uniform("uRefraction"), p.refraction);
    gl.uniform1f(this.uniform("uEdgeWidth"), Math.max(p.edgeWidth, 0.5));
    gl.uniform1f(this.uniform("uFrost"), p.blur);
    gl.uniform1f(this.uniform("uChroma"), p.chroma);
    gl.uniform1f(this.uniform("uHighlight"), p.highlight);
    gl.uniform1f(this.uniform("uRim"), p.rim);
    gl.uniform1f(this.uniform("uFresnel"), p.fresnel);
    gl.uniform1f(this.uniform("uSaturation"), p.saturation);
    gl.uniform1f(this.uniform("uBrightness"), p.brightness);
    gl.uniform4f(this.uniform("uTint"), p.tint[0], p.tint[1], p.tint[2], p.tintStrength);
    gl.uniform1f(this.uniform("uOpacity"), p.opacity);
    gl.uniform1f(this.uniform("uShadow"), p.shadow);
    gl.uniform3f(this.uniform("uFallback"), ...this.fallback);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  }

  dispose() {
    this.disposed = true;
    if (this.frame) cancelAnimationFrame(this.frame);
    const gl = this.gl;
    if (this.sharp) gl.deleteTexture(this.sharp);
    if (this.blurred) gl.deleteTexture(this.blurred);
    gl.deleteProgram(this.program);
    gl.deleteProgram(this.blurProgram);
  }
}
