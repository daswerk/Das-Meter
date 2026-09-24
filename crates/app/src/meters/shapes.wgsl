// Quads, one instance each: four corners in clip space (top-left, top-right,
// bottom-left, bottom-right) and a colour for the top and bottom edges.

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) colour: vec4<f32>,
};

@vertex
fn vs_main(
    @builtin(vertex_index) corner: u32,
    @location(0) top: vec4<f32>,
    @location(1) bottom: vec4<f32>,
    @location(2) top_colour: vec4<f32>,
    @location(3) bottom_colour: vec4<f32>,
) -> Out {
    var out: Out;
    let right = (corner & 1u) == 1u;
    if (corner & 2u) == 0u {
        out.position = vec4<f32>(select(top.xy, top.zw, right), 0.0, 1.0);
        out.colour = top_colour;
    } else {
        out.position = vec4<f32>(select(bottom.xy, bottom.zw, right), 0.0, 1.0);
        out.colour = bottom_colour;
    }
    return out;
}

@fragment
fn fs_main(in: Out) -> @location(0) vec4<f32> {
    return in.colour;
}
