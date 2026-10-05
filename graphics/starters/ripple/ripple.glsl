// Ripple: rings run out from the centre and the new scene comes up
// through them. Written for GodwinMix in the gl-transitions form.
// License: Apache-2.0

uniform float amplitude; // = 0.04
uniform float rings; // = 28.0
uniform float speed; // = 40.0

vec4 transition(vec2 uv) {
  vec2 dir = uv - vec2(0.5);
  float dist = max(length(dir), 0.0001);
  float wave = sin(dist * rings - progress * speed) * amplitude * sin(progress * 3.14159);
  vec2 at = clamp(uv + dir / dist * wave, 0.0, 1.0);
  return mix(getFromColor(at), getToColor(at), smoothstep(0.25, 0.75, progress));
}
