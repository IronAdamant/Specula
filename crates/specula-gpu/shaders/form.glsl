// The form views (sight_frame form; frame.sky_params.z): 1 clay, 2 normals, 3 curvature, 4 stripes.
// Included, under `#ifdef FORM`, by the shaders of lit surfaces (meshes, terrain) after `pc.frame`
// and `display` are declared; each returns `form_view` before it gathers any light. FORM is
// defined only by the entry points mesh_form.frag, mesh_tex_form.frag and terrain_form.frag,
// which render.rs gives a pipeline whose features have FEAT_FORM: the world's shaders hold none
// of this (behind a specialization constant, the Intel driver still charged the world's frame
// 2% for the branch).
// A surface shows nothing but its shape: its own colour, texture, metal and glow are put aside,
// and so is the world's light. What they write is meant for a frame with no tone mapping.
// `n` is the surface's own normal, not turned toward the viewer: a face whose normal points away
// (an inside-out part) is shown as one.

// A colour meant for the eye (sRGB) as the linear light the target encodes back into it.
vec3 form_linear(vec3 srgb) {
    srgb = clamp(srgb, 0.0, 1.0);
    return mix(srgb / 12.92, pow((srgb + 0.055) / 1.055, vec3(2.4)), step(vec3(0.04045), srgb));
}

vec4 form_view(float form, vec3 n, vec3 v, vec3 world) {
    vec3 c;
    // Under an orthographic camera every pixel is seen along one direction, the camera's own
    // (`v` from the camera's place, the middle of its near plane, fans out from it: clay
    // stained a ring of true outline red). An orthographic projection is told as render.rs
    // tells it for the camera's place: its bottom row is (0, 0, 0, w).
    if (pc.frame.view_proj[0][3] == 0.0 && pc.frame.view_proj[1][3] == 0.0 && pc.frame.view_proj[2][3] == 0.0) {
        vec3 r = (pc.frame.inv_view_proj * vec4(1.0, 0.0, 0.0, 0.0)).xyz;
        vec3 u = -(pc.frame.inv_view_proj * vec4(0.0, 1.0, 0.0, 0.0)).xyz;
        v = normalize(cross(r, u));
    }
    if (form < 1.5) {
        // Clay under a studio's light that comes with the camera: a key from the upper left, a
        // fill from the right and a little below, the glow of a pale ceiling and a dim floor,
        // and a soft shine, so that a surface shows which way it turns from every side.
        // The light's axes are the camera's own right and up (the same for every pixel of the
        // frame, whichever way the camera looks: taken from the world's up and each pixel's
        // own direction, four balls seen from above were lit from four sides). A direction
        // across the picture, taken back through the projection, is the camera's right (and
        // down, the picture's y running down) by a scale.
        vec3 right = normalize((pc.frame.inv_view_proj * vec4(1.0, 0.0, 0.0, 0.0)).xyz);
        vec3 up = -normalize((pc.frame.inv_view_proj * vec4(0.0, 1.0, 0.0, 0.0)).xyz);
        vec3 back = cross(right, up); // toward the camera
        // (The key rakes across more than it faces, and the fill is weak: a first look under
        // an even light showed form faintly.)
        vec3 key = normalize(back * 0.38 - right * 0.68 + up * 0.62);
        vec3 fill = normalize(back * 0.45 + right * 0.85 - up * 0.25);
        // A face whose normal points away from the viewer is lit as its other side would be,
        // and stained red: it is inside out, or the inside of an open shape.
        bool away = dot(n, v) < 0.0;
        vec3 m = away ? -n : n;
        float light = mix(0.05, 0.20, dot(m, up) * 0.5 + 0.5) + 0.95 * max(dot(m, key), 0.0) + 0.10 * max(dot(m, fill), 0.0);
        float shine = pow(max(dot(m, normalize(key + v)), 0.0), 24.0) * 0.10;
        c = (away ? vec3(0.62, 0.16, 0.12) : vec3(0.52, 0.46, 0.39)) * light + shine;
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
