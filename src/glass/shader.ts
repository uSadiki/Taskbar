// GLSL for the liquid glass. One fragment shader draws every glass surface in
// the dock window: the dock, popovers, menus and toasts are rounded-rectangle
// distance fields merged with a smooth minimum, so they flow into each other.

export const MAX_SHAPES = 8;

export const QUAD_VERTEX = /* glsl */ `#version 300 es
// One triangle that covers the viewport.
void main() {
  vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}`;

/** Separable Gaussian, used to build the frosted copy of the wallpaper. */
export const BLUR_FRAGMENT = /* glsl */ `#version 300 es
precision highp float;
uniform sampler2D uSource;
uniform vec2 uTexel;      // one texel of the source along the blur axis
uniform vec2 uSize;       // size of the render target in pixels
out vec4 fragColor;
void main() {
  vec2 uv = gl_FragCoord.xy / uSize;
  // 9-tap Gaussian using bilinear taps between texels.
  vec3 c = texture(uSource, uv).rgb * 0.2270270270;
  c += texture(uSource, uv + uTexel * 1.3846153846).rgb * 0.3162162162;
  c += texture(uSource, uv - uTexel * 1.3846153846).rgb * 0.3162162162;
  c += texture(uSource, uv + uTexel * 3.2307692308).rgb * 0.0702702703;
  c += texture(uSource, uv - uTexel * 3.2307692308).rgb * 0.0702702703;
  fragColor = vec4(c, 1.0);
}`;

export const GLASS_FRAGMENT = /* glsl */ `#version 300 es
precision highp float;

const int MAX_SHAPES = ${MAX_SHAPES};

uniform sampler2D uBackdrop;   // the wallpaper as painted on the monitor
uniform sampler2D uBlurred;    // blurred copy, with mipmaps
uniform float uHasBackdrop;
uniform vec2 uMonitor;         // monitor size, device pixels
uniform vec2 uOrigin;          // window origin on the monitor, device pixels
uniform vec2 uCanvas;          // canvas size, device pixels
uniform float uScale;          // device pixels per CSS pixel

uniform int uCount;
uniform vec4 uRect[MAX_SHAPES];     // centre.xy, half-size.xy, device pixels
uniform float uRadius[MAX_SHAPES];
uniform float uExtraFrost[MAX_SHAPES];   // added to uFrost inside that shape
uniform float uBlend;               // smooth-union distance, device pixels

uniform float uRefraction;   // largest displacement, CSS pixels
uniform float uEdgeWidth;    // bevel width, CSS pixels
uniform float uFrost;        // 0 clear .. 1 frosted
uniform float uChroma;       // colour fringing at the rim
uniform float uHighlight;    // specular on the lit rim
uniform float uRim;          // thin inner rim line
uniform float uFresnel;      // soft glow toward the edge
uniform float uSaturation;
uniform float uBrightness;
uniform vec4 uTint;          // rgb and strength
uniform float uOpacity;      // 0 glass .. 1 solid tint (opaque themes)
uniform float uShadow;
uniform vec3 uFallback;      // base colour when the wallpaper is unavailable

out vec4 fragColor;

float sdRoundBox(vec2 p, vec2 b, float r) {
  vec2 q = abs(p) - b + r;
  return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}

float smin(float a, float b, float k) {
  if (k <= 0.0) return min(a, b);
  float h = max(k - abs(a - b), 0.0) / k;
  return min(a, b) - h * h * k * 0.25;
}

// Signed distance to the union of all glass shapes: negative inside.
float scene(vec2 p) {
  float d = 1e6;
  for (int i = 0; i < MAX_SHAPES; i++) {
    if (i >= uCount) break;
    vec2 half_ = uRect[i].zw;
    float r = min(uRadius[i], min(half_.x, half_.y));
    float di = sdRoundBox(p - uRect[i].xy, half_, r);
    d = i == 0 ? di : smin(d, di, uBlend);
  }
  return d;
}

// Frost at a point: the theme's, plus the extra of the shape the point is
// in. Larger surfaces that carry text (popovers) ask for more than the dock.
// Weighted by closeness so the amount blends where shapes merge.
float frostAt(vec2 p) {
  float total = 0.0;
  float weight = 0.0;
  for (int i = 0; i < MAX_SHAPES; i++) {
    if (i >= uCount) break;
    vec2 half_ = uRect[i].zw;
    float r = min(uRadius[i], min(half_.x, half_.y));
    float di = sdRoundBox(p - uRect[i].xy, half_, r);
    float w = clamp(1.0 - di / (24.0 * uScale), 0.0, 1.0);
    w *= w;
    total += uExtraFrost[i] * w;
    weight += w;
  }
  return clamp(uFrost + (weight > 0.0 ? total / weight : 0.0), 0.0, 1.0);
}

vec3 backdrop(vec2 px, float frost) {
  vec2 uv = (uOrigin + px) / uMonitor;
  vec3 sharp = texture(uBackdrop, uv).rgb;
  if (frost <= 0.001) return sharp;
  vec3 soft = textureLod(uBlurred, uv, frost * 2.5).rgb;
  return mix(sharp, soft, smoothstep(0.0, 0.3, frost));
}

void main() {
  // Device pixels with the origin at the top-left, like the DOM.
  vec2 p = vec2(gl_FragCoord.x, uCanvas.y - gl_FragCoord.y);
  float d = scene(p);
  float inside = clamp(0.5 - d, 0.0, 1.0);   // one pixel of antialiasing

  // Optional soft shadow, cast slightly downward, only outside the glass.
  // The themes set it to zero; only the glass lab can turn it on.
  float shadow = 0.0;
  if (uShadow > 0.0) {
    float ds = scene(p - vec2(0.0, 4.0 * uScale));
    shadow = uShadow * 0.42 * exp(-max(ds, 0.0) / (10.0 * uScale)) * (1.0 - inside);
  }

  if (inside <= 0.0) {
    fragColor = vec4(0.0, 0.0, 0.0, shadow);
    return;
  }

  // Outward surface normal from the distance field's gradient.
  vec2 e = vec2(1.0, 0.0);
  vec2 n = normalize(vec2(
    scene(p + e.xy) - scene(p - e.xy),
    scene(p + e.yx) - scene(p - e.yx)) + 1e-6);

  // A thick slab with a rounded bevel: flat in the middle, steep at the rim.
  float t = clamp(-d / (uEdgeWidth * uScale), 0.0, 1.0);   // 0 at the rim, 1 on the flat
  float bevel = 1.0 - t;
  float lens = bevel * bevel;

  // Refraction: the bevel bends the view inward, so the rim shows wallpaper
  // from further inside the shape. The centre is left undistorted.
  vec2 offset = -n * lens * uRefraction * uScale;

  vec3 col;
  if (uHasBackdrop > 0.5) {
    float spread = uChroma * lens;
    float frost = frostAt(p);
    col.r = backdrop(p + offset * (1.0 + spread), frost).r;
    col.g = backdrop(p + offset, frost).g;
    col.b = backdrop(p + offset * (1.0 - spread), frost).b;
  } else {
    col = uFallback;
  }

  // Legibility: a little more colour and light, then the theme's tint.
  float luma = dot(col, vec3(0.2126, 0.7152, 0.0722));
  col = mix(vec3(luma), col, uSaturation) * uBrightness;
  col = mix(col, uTint.rgb, uTint.a);
  col = mix(col, uTint.rgb, uOpacity);

  // Light from the upper left. The lit rim gets a specular highlight, the
  // opposite rim a fainter one where light leaves the slab.
  vec2 light = normalize(vec2(-0.35, -1.0));
  float facing = dot(n, light);
  float spec = pow(max(facing, 0.0), 1.5) * pow(bevel, 3.0);
  float exit = pow(max(-facing, 0.0), 2.0) * pow(bevel, 4.0) * 0.45;
  // A one-pixel line just inside the edge, brightest where it faces the light.
  float line = 1.0 - smoothstep(0.0, 1.25 * uScale, abs(d + 0.9 * uScale));
  float lineLight = line * (0.3 + 0.7 * max(facing, 0.0) + 0.35 * max(-facing, 0.0));
  float glow = pow(bevel, 2.2) * uFresnel * 0.5;
  col += vec3(spec * uHighlight + exit * uHighlight + lineLight * uRim + glow);

  // Premultiplied alpha; the shadow shows through the antialiased edge.
  fragColor = vec4(col * inside, inside + shadow * (1.0 - inside));
}`;
