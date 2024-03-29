struct VertexInput {
    @location(0)
    pos: vec2<f32>,

    @location(1)
    tex_coords: vec2<f32>,
}

struct VertexOutput {
    @builtin(position)
    clip_pos: vec4<f32>,

    @location(1)
    tex_coords: vec2<f32>,
}

@group(0)
@binding(0)
var<uniform> time: f32;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;

    output.tex_coords = input.tex_coords;
    let sin_time = sin(time) * 0.5 + 0.5;
    output.clip_pos = vec4<f32>(input.pos * sin_time, 0.0, 1.0);

    return output;
}



struct FragmentOutput {
    @location(0)
    frag_color: vec4<f32>,
}

@group(1)
@binding(0)
var texture: texture_2d<f32>;

@group(1)
@binding(1)
var tex_sampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;

    out.frag_color = textureSample(texture, tex_sampler, in.tex_coords);

    return out;
}