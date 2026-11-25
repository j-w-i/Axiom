#version 460 core
// The soft shade of a rounded box. Where a ground is nearly black there are only a few levels between
// it and its shade, and a plain blur shows every one of them as a ring: here the steps are broken up
// by less than one level, so that none can be seen.
#include <flutter/runtime_effect.glsl>
precision highp float;

uniform vec4 uBox;     // left, top, right, bottom
uniform float uRound;
uniform float uSigma;
uniform vec4 uColor;   // not premultiplied
uniform float uGrain;  // what one level of the ground is, as a part of it

out vec4 fragColor;

vec2 erf2(vec2 x) {
  vec2 s = sign(x), a = abs(x);
  x = 1.0 + (0.278393 + (0.230389 + 0.078108 * (a * a)) * a) * a;
  x *= x;
  return s - s / (x * x);
}

float gauss(float x, float sigma) {
  return exp(-(x * x) / (2.0 * sigma * sigma)) / (2.50662827 * sigma);
}

// How much of one line of the box falls here.
float across(float x, float y, float sigma, float corner, vec2 half_) {
  float d = min(half_.y - corner - abs(y), 0.0);
  float w = half_.x - corner + sqrt(max(0.0, corner * corner - d * d));
  vec2 i = 0.5 + 0.5 * erf2((x + vec2(-w, w)) * (0.70710678 / sigma));
  return i.y - i.x;
}

float noise(vec2 p) {
  return fract(52.9829189 * fract(dot(p, vec2(0.06711056, 0.00583715))));
}

void main() {
  vec2 at = FlutterFragCoord().xy;
  vec2 half_ = (uBox.zw - uBox.xy) * 0.5;
  vec2 p = at - (uBox.xy + uBox.zw) * 0.5;
  float low = p.y - half_.y, high = p.y + half_.y;
  float start = clamp(-3.0 * uSigma, low, high), end = clamp(3.0 * uSigma, low, high);
  float step_ = (end - start) / 6.0, y = start + step_ * 0.5, v = 0.0;
  for (int i = 0; i < 6; i++) {
    v += across(p.x, p.y - y, uSigma, uRound, half_) * gauss(y, uSigma) * step_;
    y += step_;
  }
  float a = uColor.a * v;
  // Two throws, so that how rough it is does not change from step to step. It goes out with the shade.
  vec2 g = floor(at * 2.0);
  float n = noise(g) + noise(g + vec2(47.0, 17.0)) - 1.0;
  a = clamp(a + n * uGrain * smoothstep(0.0, uGrain, a), 0.0, 1.0);
  fragColor = vec4(uColor.rgb * a, a);
}
