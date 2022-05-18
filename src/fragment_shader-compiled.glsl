#version 140
#define GLSLIFY 1

in vec4 u_Color_Time;
out vec4 color;

void main() {
    float time = u_Color_Time.a;
    float time_sine = (sin(time) + 1.0f) / 2.0f;
    float time_cosine = (cos(time) + 1.0f) / 2.0f;
    color = vec4(u_Color_Time.r + time_sine, u_Color_Time.g + time_cosine, u_Color_Time.b + (time_sine + time_cosine) / 2.0f, 1.0);
}