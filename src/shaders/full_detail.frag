#version 440

/* Input compound */
in vec2 v_tex_coords;
in vec3 v_position;
in mat3 v_to_world;

/* Output */
out vec3 out_albedo;
out vec3 out_normal;
out vec3 out_position;

/* Texture samplter */
uniform sampler2D texture_atlas;
uniform sampler2D normal_atlas;
uniform bool is_shadow_pass;

void process_shadow();
void shade_standart();

void main() {
    if (is_shadow_pass) {
        process_shadow();
    } else {
        shade_standart();
    }
}

void process_shadow() {
    out_albedo = vec3(0.0);
    out_normal = vec3(0.0);
    out_position = v_position;
}

void shade_standart() {
    vec4 tex_color = texture(texture_atlas, v_tex_coords);

    /* load normal from normal map and unexponentiate it */
    vec3 local_normal = texture(normal_atlas, v_tex_coords).xyz;
    local_normal = vec3(
        pow(local_normal.x, 1.0 / (0.4545 * 0.4545)),
        pow(local_normal.y, 1.0 / (0.4545 * 0.4545)),
        pow(local_normal.z, 1.0 / (0.4545 * 0.4545))
    );

    if (tex_color.a < 0.001)
        discard;

    out_albedo = tex_color.rgb;
    out_normal = v_to_world * local_normal;
    out_position = v_position;
}