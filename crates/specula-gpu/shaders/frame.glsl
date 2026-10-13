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
};

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
