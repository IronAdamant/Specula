// Shared declarations: per-frame data, per-draw records and the global descriptor set.
// Included by the stage sources (glslc resolves #include); not compiled on its own.
#extension GL_EXT_buffer_reference : require

// Per-frame data; must match `FrameData` in src/render.rs.
struct PointLight {
    vec4 pos_range; // xyz world position, w range (m)
    vec4 color;     // linear RGB x intensity
};
layout(buffer_reference, std430, buffer_reference_align = 16) readonly buffer Frame {
    mat4 view_proj;
    vec4 camera_time;   // xyz camera, w time (s)
    vec4 sun_dir;       // xyz towards the sun
    vec4 sun_color;     // linear RGB x intensity
    vec4 ambient;       // linear RGB x intensity, w exposure
    vec4 sky;           // linear clear colour, w tone mapping on
    vec4 viewport;      // width, height, light count
    PointLight lights[16];
    mat4 inv_view_proj;
    mat4 shadow_mats[4]; // world -> shadow clip per cascade
    vec4 shadow_params; // cascades, filter radius (texels of cascade 0), normal offset (texels)
    vec4 shadow_texel;  // world size of a texel per cascade
    vec4 shadow_axis;   // view depth = dot(xyz, p) + w
    vec4 shadow_fade;   // view depths where shadows start / finish fading out
    vec4 fog_color;     // linear RGB, w density (per m at the reference height)
    vec4 fog_params;    // falloff (per m), reference height, start distance, sun scatter
    vec4 sky_zenith;    // linear RGB x intensity, w cos(disc radius)
    vec4 sky_horizon;   // linear RGB x intensity, w disc intensity
    vec4 sky_ground;    // linear RGB x intensity, w glow strength
    vec4 sky_params;    // horizon sharpness
    vec4 grade_balance; // white balance LMS gains, w contrast
    vec4 grade_lift;    // lift per channel, w saturation
    vec4 grade_gamma;   // 1 / gamma per channel, w vibrance
    vec4 grade_gain;    // gain per channel, w LUT strength
    vec4 grade_fx;      // vignette, vignette radius, grain, unused
    uvec4 post_slots;   // LUT slot, LUT size, SSAO slot, grading steps (src/post.rs)
};

// Fraction of light that crosses the height fog from `dist` metres along `dir` to the camera
// (`to_infinity` for the sky: then the ray must climb out of the fog or it is fully fogged).
float fog_transmittance(Frame f, vec3 dir, float dist, bool to_infinity) {
    float start = f.fog_params.z;
    if (!to_infinity && dist <= start) {
        return 1.0;
    }
    vec3 o = f.camera_time.xyz + dir * start;
    float density = f.fog_color.w;
    float falloff = f.fog_params.x;
    float k = density * exp(clamp(-falloff * (o.y - f.fog_params.y), -80.0, 80.0));
    float optical;
    if (to_infinity) {
        if (falloff * dir.y <= 1e-5) {
            return 0.0;
        }
        optical = k / (falloff * dir.y);
    } else {
        float len = dist - start;
        float x = falloff * dir.y * len;
        optical = abs(x) > 1e-4 ? k * len * (1.0 - exp(clamp(-x, -80.0, 80.0))) / x : k * len;
    }
    return exp(-optical);
}

// The fog's colour seen along `dir`: brighter towards the sun.
vec3 fog_radiance(Frame f, vec3 dir) {
    float s = pow(max(dot(dir, f.sun_dir.xyz), 0.0), 8.0);
    return f.fog_color.rgb + f.sun_color.rgb * (f.fog_params.w * s);
}

// Per-draw records; must match `DrawRecord` in src/render.rs.
struct Draw {
    mat4 model;  // object -> world
    vec4 color;  // linear RGB, a opacity
    vec4 color2; // linear RGB, a seed
    vec4 params; // emissive, metallic, roughness, specular
};
layout(buffer_reference, std430, buffer_reference_align = 16) readonly buffer Draws {
    Draw items[];
};
