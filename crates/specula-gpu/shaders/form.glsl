// The form views (sight_frame form; frame.sky_params.z): 1 clay, 2 normals, 3 curvature, 4 stripes.
// Included by the shaders of lit surfaces (meshes, terrain) after `pc.frame` and `display` are
// declared; each returns `form_view` before it gathers any light.
// A surface shows nothing but its shape: its own colour, texture, metal and glow are put aside,
// and so is the world's light. What they write is meant for a frame with no tone mapping.

// A colour meant for the eye (sRGB) as the linear light the target encodes back into it.
vec3 form_linear(vec3 srgb) {
    srgb = clamp(srgb, 0.0, 1.0);
    return mix(srgb / 12.92, pow((srgb + 0.055) / 1.055, vec3(2.4)), step(vec3(0.04045), srgb));
}

vec4 form_view(float form, vec3 n, vec3 v, vec3 world) {
    vec3 c;
    if (form < 1.5) {
        // Clay under a studio's light that comes with the camera: a key from the upper left, a
        // fill from the right and a little below, the glow of a pale ceiling and a dim floor,
        // and a soft shine, so that a surface shows which way it turns from every side.
        vec3 right = cross(vec3(0.0, 1.0, 0.0), v);
        float rl = dot(right, right);
        right = rl > 1e-6 ? right * inversesqrt(rl) : vec3(1.0, 0.0, 0.0);
        vec3 up = cross(v, right);
        // (The key rakes across more than it faces, and the fill is weak: a first look under
        // an even light showed form faintly.)
        vec3 key = normalize(v * 0.38 - right * 0.68 + up * 0.62);
        vec3 fill = normalize(v * 0.45 + right * 0.85 - up * 0.25);
        float light = mix(0.05, 0.20, dot(n, up) * 0.5 + 0.5) + 0.95 * max(dot(n, key), 0.0) + 0.10 * max(dot(n, fill), 0.0);
        float shine = pow(max(dot(n, normalize(key + v)), 0.0), 24.0) * 0.10;
        c = vec3(0.52, 0.46, 0.39) * light + shine;
    } else if (form < 2.5) {
        // Normals: where the surface faces, world x, y, z as red, green, blue.
        c = form_linear(n * 0.5 + 0.5);
    } else if (form < 3.5) {
        // Curvature: how far the normal turns for each metre across the surface, here: flat is
        // mid grey, turning away (a ridge, a ball) lighter, turning in (a groove) darker. A
        // ball of the scale's radius (frame.sky_params.w) is halfway from mid grey to white,
        // one a tenth of that radius nearly white: the turn is eased, never cut off, so that
        // a limb and a finger on it both show (cut off at white, a first look lost both).
        vec3 px = dFdx(world), py = dFdy(world);
        float turn = (dot(dFdx(n), px) + dot(dFdy(n), py)) / max(dot(px, px) + dot(py, py), 1e-12) * pc.frame.sky_params.w;
        c = form_linear(vec3(0.5 + 0.5 * turn / (1.0 + abs(turn))));
    } else {
        // Stripes: what a mirror of this shape shows of a room of level bands. Even bands on an
        // even surface; where they kink or pinch, so does the surface.
        // Where the bands would be narrower than a few pixels (a finger, a toe) they fade to
        // mid grey: drawn, they are a shimmer that says nothing.
        float at = reflect(-v, n).y * 6.0;
        float band = abs(fract(at) - 0.5) * 2.0;
        float fine = fwidth(at);
        float edge = clamp(fine * 2.0, 1e-3, 0.5);
        float stripe = mix(0.08, 0.92, smoothstep(0.5 - edge, 0.5 + edge, band));
        c = form_linear(vec3(mix(stripe, 0.5, smoothstep(0.2, 0.5, fine))));
    }
    return vec4(display(c / max(pc.frame.ambient.w, 1e-6)), 1.0);
}
