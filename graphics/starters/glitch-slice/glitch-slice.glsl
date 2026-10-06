// Glitch slice: the picture breaks into bands that jump sideways, and each
// band changes to the new scene at its own moment. Written for GodwinMix in
// the gl-transitions form.
// License: Apache-2.0

uniform float bands; // = 24.0
uniform float shift; // = 0.12

float hash(float n) {
  return fract(sin(n * 12.9898) * 43758.5453);
}

vec4 transition(vec2 uv) {
  float row = floor((1.0 - uv.y) * bands);
  float jump = floor(progress * 12.0);
  float by = (hash(row + jump * 7.0) - 0.5) * 2.0 * shift * sin(progress * 3.14159);
  vec2 at = vec2(fract(uv.x + by), uv.y);
  float when = hash(row * 3.1) * 0.8 + 0.1;
  return progress < when ? getFromColor(at) : getToColor(at);
}
